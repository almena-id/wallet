//! Presenting held credentials to a registry's form (`registry.rs`,
//! `purpose: "present"`): an OpenID4VP request whose DCQL query names, per
//! credential, a format, the `vct`s it takes and the claims it wants.
//!
//! **What goes.** For each credential set of the query, the first of its
//! options this wallet can answer: an SD-JWT VC (`dc+sd-jwt`) it holds, still
//! valid, whose `vct` the query names. Of it only the disclosures the query
//! asks for are sent — selective disclosure — and nothing else this wallet
//! holds. The sheet shows exactly that before anything is sent, and the same
//! choice is made again on Accept from the same credentials.
//!
//! **Bound to the holder.** Each presentation ends with a key-binding JWT
//! (`typ: kb+jwt`): `iat`, the request's `aud` and `nonce`, and `sd_hash` (the
//! SHA-256 of the SD-JWT as presented), signed by the key the credential is
//! bound to — derived again from the seed and the `scope` kept with it, never
//! stored.

use base64::engine::general_purpose::URL_SAFE_NO_PAD as B64;
use base64::Engine as _;
use ed25519_dalek::{Signer, SigningKey};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use crate::credentials::Credential;

/// One credential chosen for one query: the query it answers, the credential
/// and the claims disclosed from it.
#[derive(Debug, Clone)]
pub struct Chosen {
    pub query: String,
    pub credential: Credential,
    /// Disclosed, by name and value, in the credential's order.
    pub claims: Vec<(String, Value)>,
}

fn decode(part: &str) -> Option<Value> {
    B64.decode(part)
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
}

/// The credential's `vct`, from its issuer's payload.
fn vct_of(credential: &str) -> Option<String> {
    let jws = credential.split('~').next()?;
    decode(jws.split('.').nth(1)?)?["vct"]
        .as_str()
        .map(str::to_owned)
}

/// The claim names a credential query asks for: the first step of each path.
fn asked(query: &Value) -> Vec<String> {
    query["claims"]
        .as_array()
        .map(|claims| {
            claims
                .iter()
                .filter_map(|claim| claim["path"][0].as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default()
}

/// A held credential that answers `query`, if any: the format and a `vct` it
/// names, still valid at `now`.
fn answering<'a>(query: &Value, held: &'a [Credential], now: u64) -> Option<&'a Credential> {
    if query["format"].as_str() != Some("dc+sd-jwt") {
        return None;
    }
    let wanted: Vec<&str> = query["meta"]["vct_values"]
        .as_array()?
        .iter()
        .filter_map(Value::as_str)
        .collect();
    held.iter().find(|credential| {
        credential.format == "dc+sd-jwt"
            && credential.valid_until > now
            && vct_of(&credential.credential).is_some_and(|vct| wanted.contains(&vct.as_str()))
    })
}

/// What this wallet would present for `dcql`, and whether every required
/// credential set is answered.
pub fn choose(dcql: &Value, held: &[Credential], now: u64) -> (Vec<Chosen>, bool) {
    let queries: Vec<&Value> = dcql["credentials"]
        .as_array()
        .map(|all| all.iter().collect())
        .unwrap_or_default();
    let by_id = |id: &str| {
        queries
            .iter()
            .copied()
            .find(|q| q["id"].as_str() == Some(id))
    };
    let mut chosen = Vec::new();
    let mut complete = true;
    for set in dcql["credential_sets"]
        .as_array()
        .cloned()
        .unwrap_or_default()
    {
        let options = set["options"].as_array().cloned().unwrap_or_default();
        // Each option is a list of query ids; here, one each.
        let picked = options.iter().find_map(|option| {
            let id = option[0].as_str()?;
            let query = by_id(id)?;
            answering(query, held, now).map(|credential| (id.to_owned(), query, credential))
        });
        match picked {
            Some((id, query, credential)) => {
                let wanted = asked(query);
                let claims = credential
                    .claims
                    .iter()
                    .filter(|(name, _)| wanted.contains(name))
                    .map(|(name, value)| (name.clone(), value.clone()))
                    .collect();
                chosen.push(Chosen {
                    query: id,
                    credential: credential.clone(),
                    claims,
                });
            }
            None if set["required"].as_bool().unwrap_or(true) => complete = false,
            None => {}
        }
    }
    (chosen, complete)
}

/// The SD-JWT as presented: the issuer's JWS and only the disclosures of the
/// claims named.
fn selected(credential: &str, names: &[String]) -> String {
    let mut parts = credential.split('~');
    let jws = parts.next().unwrap_or_default();
    let mut out = format!("{jws}~");
    for disclosure in parts.filter(|part| !part.is_empty()) {
        let name = decode(disclosure).and_then(|value| value[1].as_str().map(str::to_owned));
        if name.is_some_and(|name| names.contains(&name)) {
            out.push_str(disclosure);
            out.push('~');
        }
    }
    out
}

/// The key-binding JWT over `presented`, for `audience` and `nonce`.
fn key_binding(key: &SigningKey, presented: &str, audience: &str, nonce: &str, now: u64) -> String {
    let header = json!({"alg": "EdDSA", "typ": "kb+jwt"});
    let payload = json!({
        "iat": now,
        "aud": audience,
        "nonce": nonce,
        "sd_hash": B64.encode(Sha256::digest(presented.as_bytes())),
    });
    let input = format!(
        "{}.{}",
        B64.encode(header.to_string()),
        B64.encode(payload.to_string())
    );
    let signature = key.sign(input.as_bytes());
    format!("{input}.{}", B64.encode(signature.to_bytes()))
}

/// The `vp_token`: for each query answered, its one presentation, each bound
/// with the key `key_for` derives from the credential's scope.
pub fn vp_token(
    chosen: &[Chosen],
    key_for: impl Fn(&str) -> SigningKey,
    audience: &str,
    nonce: &str,
    now: u64,
) -> Value {
    let mut token = serde_json::Map::new();
    for one in chosen {
        let names: Vec<String> = one.claims.iter().map(|(name, _)| name.clone()).collect();
        let presented = selected(&one.credential.credential, &names);
        let key = key_for(&one.credential.scope);
        let bound = key_binding(&key, &presented, audience, nonce, now);
        token.insert(one.query.clone(), json!([format!("{presented}{bound}")]));
    }
    Value::Object(token)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signature, Verifier};
    use std::collections::BTreeMap;

    const VCT: &str = "https://almena.id/credentials/membership/v1";

    fn disclosure(name: &str, value: &str) -> String {
        B64.encode(json!(["salt", name, value]).to_string())
    }

    fn held(valid_until: u64) -> Credential {
        let disclosures = [
            disclosure("given_name", "Lucía"),
            disclosure("member_number", "0042"),
        ];
        let payload = json!({"vct": VCT, "iss": "did:webvh:Qm:almena.id:ids:idn_club"});
        let jws = format!(
            "{}.{}.sig",
            B64.encode("{}"),
            B64.encode(payload.to_string())
        );
        Credential {
            id: "x".into(),
            format: "dc+sd-jwt".into(),
            credential: format!("{jws}~{}~{}~", disclosures[0], disclosures[1]),
            issuer_did: "did:webvh:Qm:almena.id:ids:idn_club".into(),
            issuer_name: "Club".into(),
            type_id: "membership".into(),
            type_labels: BTreeMap::new(),
            claims: BTreeMap::from([
                ("given_name".to_owned(), json!("Lucía")),
                ("member_number".to_owned(), json!("0042")),
            ]),
            issued_at: 1,
            valid_until,
            received_at: 1,
            scope: "https://api.almena.id did:webvh:Qm:almena.id:ids:idn_club".into(),
        }
    }

    fn dcql(required: bool) -> Value {
        json!({
            "credentials": [
                {"id": "membership_sd_jwt", "format": "dc+sd-jwt",
                 "meta": {"vct_values": [VCT]}, "claims": [{"path": ["member_number"]}]},
                {"id": "membership_w3c", "format": "jwt_vc_json", "meta": {}, "claims": []},
                {"id": "pid_sd_jwt", "format": "dc+sd-jwt",
                 "meta": {"vct_values": ["urn:eudi:pid:1"]}, "claims": [{"path": ["birthdate"]}]},
            ],
            "credential_sets": [
                {"options": [["membership_sd_jwt"], ["membership_w3c"]], "required": true},
                {"options": [["pid_sd_jwt"]], "required": required},
            ],
        })
    }

    #[test]
    fn only_what_is_asked_of_a_valid_credential_is_chosen() {
        let (chosen, complete) = choose(&dcql(false), &[held(10)], 5);
        assert!(complete);
        assert_eq!(chosen.len(), 1);
        assert_eq!(chosen[0].query, "membership_sd_jwt");
        assert_eq!(
            chosen[0].claims,
            vec![("member_number".to_owned(), json!("0042"))]
        );
        // A required set this wallet cannot answer, and an expired credential.
        assert!(!choose(&dcql(true), &[held(10)], 5).1);
        let (none, complete) = choose(&dcql(false), &[held(4)], 5);
        assert!(none.is_empty() && !complete);
    }

    #[test]
    fn the_presentation_discloses_only_the_chosen_claims_and_is_bound() {
        let key = SigningKey::from_bytes(&[9; 32]);
        let (chosen, _) = choose(&dcql(false), &[held(10)], 5);
        let token = vp_token(
            &chosen,
            |_| key.clone(),
            "https://registry.almena.id",
            "n",
            5,
        );
        let presented = token["membership_sd_jwt"][0].as_str().unwrap();
        let (sd_jwt, binding) = presented.rsplit_once('~').unwrap();
        let sd_jwt = format!("{sd_jwt}~");
        assert!(sd_jwt.contains(&disclosure("member_number", "0042")));
        assert!(!sd_jwt.contains(&disclosure("given_name", "Lucía")));
        let mut parts = binding.split('.');
        let (header, payload) = (parts.next().unwrap(), parts.next().unwrap());
        let signature = B64.decode(parts.next().unwrap()).unwrap();
        key.verifying_key()
            .verify(
                format!("{header}.{payload}").as_bytes(),
                &Signature::from_slice(&signature).unwrap(),
            )
            .expect("bound");
        let claims = decode(payload).unwrap();
        assert_eq!(claims["nonce"], json!("n"));
        assert_eq!(claims["aud"], json!("https://registry.almena.id"));
        assert_eq!(
            claims["sd_hash"],
            json!(B64.encode(Sha256::digest(sd_jwt.as_bytes())))
        );
    }
}
