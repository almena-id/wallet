//! Resolving a `did:webvh` (1.0) by walking its log: the DID document it ends
//! in, believed only once every entry has been checked from the first.
//!
//! A `did:webvh:{SCID}:{host}[:path]` keeps its history at
//! `https://{host}/{path}/did.jsonl` (`/.well-known/did.jsonl` for a bare
//! host): one entry per line, `{versionId, versionTime, parameters, state,
//! proof}`. Walking it, every entry must hold:
//!
//! - **the SCID**: the first entry, without its proof, its `versionId` set to
//!   the SCID and every SCID written back as `{SCID}`, hashes (SHA-256 over
//!   its RFC 8785 form, as a base58btc multihash) to the SCID the DID names;
//!   it names the method (`did:webvh:1.0`), that SCID and its `updateKeys`;
//! - **the chain**: `versionId` is `{n}-{hash}`, `n` counting from 1, and the
//!   hash is the entry's without its proof, with the previous `versionId` in
//!   place of its own (the SCID, for the first);
//! - **the proof**: an `eddsa-jcs-2022` Data Integrity proof
//!   (`assertionMethod`) by one of the update keys in force before it — the
//!   first entry's own, for the first — named as its `did:key`;
//! - **pre-rotation**: once `nextKeyHashes` are set, new `updateKeys` must
//!   each hash to one of them;
//! - **the rest**: `versionTime` never goes back, the method and SCID never
//!   change, `state.id` is the DID itself, and nothing follows a
//!   deactivation. A deactivated DID resolves to nothing.
//!
//! Witnesses are not supported: a log that asks for them is refused rather
//! than half believed. Whatever does not hold refuses the whole log.

use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};

use crate::registry::jcs;

const METHOD: &str = "did:webvh:1.0";
const PLACEHOLDER: &str = "{SCID}";
const CRYPTOSUITE: &str = "eddsa-jcs-2022";
const ED25519_PUB: [u8; 2] = [0xed, 0x01];

/// SHA-256 as a base58btc multihash (`Qm…`): what SCIDs, entry hashes and
/// next key hashes are.
pub fn multihash(data: &[u8]) -> String {
    let mut bytes = vec![0x12, 0x20];
    bytes.extend_from_slice(&Sha256::digest(data));
    bs58::encode(bytes).into_string()
}

/// Where a `did:webvh`'s log is: `https://{host}/{path}/did.jsonl`.
pub fn log_url(did: &str) -> Option<String> {
    let rest = did.strip_prefix("did:webvh:")?;
    let mut parts = rest.split(':');
    let _scid = parts.next().filter(|scid| !scid.is_empty())?;
    let host = parts
        .next()
        .filter(|host| !host.is_empty())?
        .replace("%3A", ":")
        .replace("%3a", ":");
    let path: Vec<&str> = parts.collect();
    if host.contains(['/', '?', '#', '@']) || path.iter().any(|part| part.is_empty()) {
        return None;
    }
    Some(if path.is_empty() {
        format!("https://{host}/.well-known/did.jsonl")
    } else {
        format!("https://{host}/{}/did.jsonl", path.join("/"))
    })
}

fn scid_of(did: &str) -> Option<&str> {
    did.strip_prefix("did:webvh:")?.split(':').next()
}

fn hash_of(value: &Value) -> Option<String> {
    Some(multihash(jcs(value).ok()?.as_bytes()))
}

fn without_proof(entry: &Map<String, Value>) -> Map<String, Value> {
    entry
        .iter()
        .filter(|(name, _)| name.as_str() != "proof")
        .map(|(name, value)| (name.clone(), value.clone()))
        .collect()
}

fn strings(value: &Value) -> Option<Vec<String>> {
    value
        .as_array()?
        .iter()
        .map(|item| item.as_str().map(str::to_owned))
        .collect()
}

fn ed25519(multikey: &str) -> Option<VerifyingKey> {
    let bytes = bs58::decode(multikey.strip_prefix('z')?).into_vec().ok()?;
    let raw = bytes.strip_prefix(&ED25519_PUB)?;
    VerifyingKey::from_bytes(raw.try_into().ok()?).ok()
}

/// Whether `proof` is an `eddsa-jcs-2022` signature over `entry` (its proof
/// left out) by one of `signers`, named as that key's `did:key`.
fn proves(entry: &Map<String, Value>, proof: &Value, signers: &[String]) -> bool {
    let check = || -> Option<()> {
        let options = proof.as_object()?;
        let method = options.get("verificationMethod")?.as_str()?;
        let value = options.get("proofValue")?.as_str()?;
        let (named, key) = method.split_once('#')?;
        if options.get("type")?.as_str()? != "DataIntegrityProof"
            || options.get("cryptosuite")?.as_str()? != CRYPTOSUITE
            || options.get("proofPurpose")?.as_str()? != "assertionMethod"
            || named != format!("did:key:{key}")
            || !signers.iter().any(|signer| signer == key)
            // An entry has no context, so its proof options carry none.
            || options.contains_key("@context") != entry.contains_key("@context")
        {
            return None;
        }
        let mut unsigned = options.clone();
        unsigned.remove("proofValue");
        let mut hashed = Sha256::digest(jcs(&Value::Object(unsigned)).ok()?.as_bytes()).to_vec();
        hashed.extend_from_slice(&Sha256::digest(
            jcs(&Value::Object(without_proof(entry))).ok()?.as_bytes(),
        ));
        let signature =
            Signature::from_slice(&bs58::decode(value.strip_prefix('z')?).into_vec().ok()?).ok()?;
        ed25519(key)?.verify(&hashed, &signature).ok()
    };
    check().is_some()
}

/// The DID document `did`'s log ends in, if every entry of `log` (JSON Lines,
/// oldest first) holds; `None` for anything else, a deactivated DID included.
pub fn resolve(log: &str, did: &str) -> Option<Value> {
    let scid = scid_of(did).filter(|scid| !scid.is_empty())?;
    let entries: Vec<Map<String, Value>> = log
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| match serde_json::from_str(line) {
            Ok(Value::Object(entry)) => Some(entry),
            _ => None,
        })
        .collect::<Option<_>>()?;
    let mut update_keys: Vec<String> = Vec::new();
    let mut next_hashes: Vec<String> = Vec::new();
    let mut previous: Option<String> = None;
    let mut time = String::new();
    let mut deactivated = false;
    let mut state = None;

    for (at, entry) in entries.iter().enumerate() {
        if deactivated {
            return None;
        }
        let version_id = entry.get("versionId")?.as_str()?;
        let (number, hash) = version_id.split_once('-')?;
        if number.parse::<usize>().ok()? != at + 1 {
            return None;
        }
        let parameters = entry.get("parameters")?.as_object()?;
        let first = previous.is_none();

        // The method and the SCID: named by the first entry, never changed.
        if parameters
            .get("method")
            .is_some_and(|method| method != METHOD)
            || parameters
                .get("scid")
                .is_some_and(|named| named.as_str() != Some(scid))
            || (first && (parameters.get("method").is_none() || parameters.get("scid").is_none()))
        {
            return None;
        }
        if parameters.get("witness").is_some_and(|witness| {
            !witness.is_null() && witness.as_object().is_none_or(|w| !w.is_empty())
        }) {
            return None;
        }

        // The chain: hashed with the previous versionId (the SCID, first).
        let mut chained = without_proof(entry);
        let before = previous.clone().unwrap_or_else(|| scid.to_owned());
        chained.insert("versionId".into(), Value::String(before));
        let chained = Value::Object(chained);
        if hash_of(&chained)? != hash {
            return None;
        }
        if first {
            let text = jcs(&chained).ok()?.replace(scid, PLACEHOLDER);
            if multihash(text.as_bytes()) != scid {
                return None;
            }
        }

        // Who may sign it: the keys in force before it; the first, its own.
        let named_keys = match parameters.get("updateKeys") {
            Some(keys) => Some(strings(keys)?),
            None => None,
        };
        let signers = if first {
            named_keys.clone().filter(|keys| !keys.is_empty())?
        } else {
            update_keys.clone()
        };
        let proofs = match entry.get("proof")? {
            Value::Array(items) => items.clone(),
            single => vec![single.clone()],
        };
        if !proofs.iter().any(|proof| proves(entry, proof, &signers)) {
            return None;
        }

        // Pre-rotation: new update keys were committed to beforehand.
        if let Some(keys) = &named_keys {
            if !next_hashes.is_empty()
                && !keys
                    .iter()
                    .all(|key| next_hashes.contains(&multihash(key.as_bytes())))
            {
                return None;
            }
            update_keys = keys.clone();
        }
        if let Some(hashes) = parameters.get("nextKeyHashes") {
            next_hashes = strings(hashes)?;
        }

        let when = entry.get("versionTime")?.as_str()?;
        if when < time.as_str() {
            return None;
        }
        time = when.to_owned();
        deactivated = parameters.get("deactivated") == Some(&Value::Bool(true));
        let document = entry.get("state")?;
        if document.get("id")?.as_str()? != did {
            return None;
        }
        state = Some(document.clone());
        previous = Some(version_id.to_owned());
    }
    if deactivated {
        return None;
    }
    state
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::identity::keys;
    use ed25519_dalek::SigningKey;
    use serde_json::json;

    /// A log entry signed by `key` with an `eddsa-jcs-2022` proof.
    fn sign(entry: Value, key: &SigningKey) -> Value {
        let multikey = keys::written(&key.verifying_key().to_bytes());
        let proof = crate::registry::proof(
            key,
            &format!("did:key:{multikey}"),
            "assertionMethod",
            &entry,
            "2026-10-01T00:00:00Z",
        )
        .expect("proof");
        let mut signed = entry;
        signed["proof"] = json!([proof]);
        signed
    }

    /// The first entry of a DID at `location` (`host:path`), as the registry
    /// makes it: the SCID hashed with placeholders, then put in place.
    pub fn genesis(location: &str, key: &SigningKey, state: Value) -> (String, Value) {
        let multikey = keys::written(&key.verifying_key().to_bytes());
        let template = format!("did:webvh:{PLACEHOLDER}:{location}");
        let mut document = state;
        document["id"] = json!(template);
        let preliminary = json!({
            "versionId": PLACEHOLDER,
            "versionTime": "2026-10-01T00:00:00Z",
            "parameters": {"method": METHOD, "scid": PLACEHOLDER, "updateKeys": [multikey]},
            "state": document,
        });
        let scid = multihash(jcs(&preliminary).unwrap().as_bytes());
        let mut entry: Value =
            serde_json::from_str(&preliminary.to_string().replace(PLACEHOLDER, &scid)).unwrap();
        let hash = multihash(jcs(&entry).unwrap().as_bytes());
        entry["versionId"] = json!(format!("1-{hash}"));
        (format!("did:webvh:{scid}:{location}"), sign(entry, key))
    }

    /// The entry after `previous`, signed by `key`.
    pub fn following(previous: &Value, parameters: Value, state: Value, key: &SigningKey) -> Value {
        let before = previous["versionId"].as_str().unwrap();
        let number: u64 = before.split('-').next().unwrap().parse().unwrap();
        let mut entry = json!({
            "versionId": before,
            "versionTime": "2026-10-02T00:00:00Z",
            "parameters": parameters,
            "state": state,
        });
        let hash = multihash(jcs(&entry).unwrap().as_bytes());
        entry["versionId"] = json!(format!("{}-{hash}", number + 1));
        sign(entry, key)
    }

    pub fn lines(entries: &[Value]) -> String {
        entries.iter().map(|entry| format!("{entry}\n")).collect()
    }

    fn key(seed: u8) -> SigningKey {
        SigningKey::from_bytes(&[seed; 32])
    }

    #[test]
    fn a_log_resolves_to_its_last_state_once_every_entry_holds() {
        let owner = key(1);
        let (did, first) = genesis("almena.id:ids:idn_club", &owner, json!({}));
        let mut state = first["state"].clone();
        state["assertionMethod"] = json!([format!("{did}#zKey")]);
        let second = following(&first, json!({}), state.clone(), &owner);
        let resolved = resolve(&lines(&[first.clone(), second.clone()]), &did).expect("resolves");
        assert_eq!(resolved, state);
        assert_eq!(
            resolve(&lines(std::slice::from_ref(&first)), &did),
            Some(first["state"].clone())
        );

        // Another DID's log, a broken chain, entries out of order.
        let (other, _) = genesis("almena.id:ids:idn_other", &owner, json!({}));
        assert!(resolve(&lines(std::slice::from_ref(&first)), &other).is_none());
        assert!(resolve(&lines(std::slice::from_ref(&second)), &did).is_none());
        assert!(resolve(&lines(&[second.clone(), first.clone()]), &did).is_none());
        // Any change after signing.
        let mut altered = second.clone();
        altered["state"]["assertionMethod"] = json!([format!("{did}#zOther")]);
        assert!(resolve(&lines(&[first.clone(), altered]), &did).is_none());
        let mut renamed = first.clone();
        renamed["parameters"]["updateKeys"] =
            json!([keys::written(&key(2).verifying_key().to_bytes())]);
        assert!(resolve(&lines(&[renamed]), &did).is_none());
        assert!(resolve("", &did).is_none());
        assert!(resolve("not json\n", &did).is_none());
    }

    #[test]
    fn only_the_update_keys_in_force_sign_the_next_entry() {
        let owner = key(1);
        let heir = key(2);
        let heir_key = keys::written(&heir.verifying_key().to_bytes());
        let (did, first) = genesis("almena.id:ids:idn_club", &owner, json!({}));
        let state = first["state"].clone();
        // Not by a key that is not in force yet.
        let usurped = following(&first, json!({}), state.clone(), &heir);
        assert!(resolve(&lines(&[first.clone(), usurped]), &did).is_none());
        // Handed over by the owner, then signed by the heir.
        let handover = following(
            &first,
            json!({"updateKeys": [heir_key]}),
            state.clone(),
            &owner,
        );
        let next = following(&handover, json!({}), state.clone(), &heir);
        assert!(resolve(&lines(&[first.clone(), handover.clone(), next]), &did).is_some());
        // The owner's key is no longer in force.
        let stale = following(&handover, json!({}), state.clone(), &owner);
        assert!(resolve(&lines(&[first, handover, stale]), &did).is_none());
    }

    #[test]
    fn pre_rotation_deactivation_and_witnesses_are_held_to() {
        let owner = key(1);
        let heir = key(2);
        let heir_key = keys::written(&heir.verifying_key().to_bytes());
        let stranger_key = keys::written(&key(3).verifying_key().to_bytes());
        let (did, first) = genesis("almena.id:ids:idn_club", &owner, json!({}));
        let state = first["state"].clone();
        let committed = following(
            &first,
            json!({"nextKeyHashes": [multihash(heir_key.as_bytes())]}),
            state.clone(),
            &owner,
        );
        let kept = following(
            &committed,
            json!({"updateKeys": [heir_key]}),
            state.clone(),
            &owner,
        );
        assert!(resolve(&lines(&[first.clone(), committed.clone(), kept]), &did).is_some());
        let broken = following(
            &committed,
            json!({"updateKeys": [stranger_key]}),
            state.clone(),
            &owner,
        );
        assert!(resolve(&lines(&[first.clone(), committed, broken]), &did).is_none());

        let ended = following(&first, json!({"deactivated": true}), state.clone(), &owner);
        assert!(resolve(&lines(&[first.clone(), ended]), &did).is_none());
        let witnessed = following(
            &first,
            json!({"witness": {"threshold": 1, "witnesses": [{"id": "did:key:zW"}]}}),
            state,
            &owner,
        );
        assert!(resolve(&lines(&[first, witnessed]), &did).is_none());
    }

    /// A log made by the registry's own code (`api`'s `webvh.py`: genesis,
    /// a handover of the update key, an entry by the new key), signed as
    /// wallets sign — so the two implementations are held to each other.
    const REGISTRY_LOG: &str = include_str!("fixtures/registry-did.jsonl");
    const REGISTRY_DID: &str =
        "did:webvh:QmQi5rPBEy17S5sNgnpM8dFZdSdqrbt3JF4t2G1bDP9kn1:almena.id:ids:idn_club";

    #[test]
    fn a_log_the_registry_wrote_resolves() {
        let state = resolve(REGISTRY_LOG, REGISTRY_DID).expect("resolves");
        assert_eq!(
            state["assertionMethod"][0],
            json!(format!(
                "{REGISTRY_DID}#z6Mko9hTggMwjSTEaJaPUfE6tqcy2xvU6BnNq3e3o8qVBiyH"
            ))
        );
        // Without its last entry it ends earlier; without its first, nowhere.
        let lines: Vec<&str> = REGISTRY_LOG.lines().collect();
        assert!(resolve(&lines[..2].join("\n"), REGISTRY_DID).is_some());
        assert!(resolve(&lines[1..].join("\n"), REGISTRY_DID).is_none());
    }

    #[test]
    fn a_did_says_where_its_log_is() {
        assert_eq!(
            log_url("did:webvh:QmS:almena.id:ids:idn_club").as_deref(),
            Some("https://almena.id/ids/idn_club/did.jsonl")
        );
        assert_eq!(
            log_url("did:webvh:QmS:localhost%3A8000").as_deref(),
            Some("https://localhost:8000/.well-known/did.jsonl")
        );
        assert_eq!(log_url("did:web:almena.id"), None);
        assert_eq!(log_url("did:webvh:QmS:almena.id::x"), None);
    }
}
