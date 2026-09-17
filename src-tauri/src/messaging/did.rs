//! Resolving the DIDs a conversation names: `did:key` by derivation, with
//! nothing fetched, and `did:web` over HTTPS. Two methods, because those are
//! the two the platform's people and entities use; a third is not resolved
//! until the spec adopts it.

use std::time::Duration;

use didcomm::did::{
    DIDCommMessagingService, DIDDoc, DIDResolver, Service, ServiceKind, VerificationMaterial,
    VerificationMethod, VerificationMethodType,
};
use didcomm::error::{Error, ErrorKind, Result as DidcommResult};
use ed25519_dalek::VerifyingKey;
use serde_json::{json, Value};

use crate::identity::keys::{multibase, ED25519_PUB, X25519_PUB};

/// The DIDComm library's way in. Cloned freely: the HTTP client inside is a
/// handle.
#[derive(Clone)]
pub struct Resolver {
    client: reqwest::Client,
}

impl Resolver {
    pub fn new() -> Self {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(15))
            // The method allows a temporary redirect only; a permanent move is
            // a document update rather than something to follow.
            .redirect(reqwest::redirect::Policy::custom(|attempt| {
                if attempt.status() == reqwest::StatusCode::TEMPORARY_REDIRECT
                    && attempt.previous().len() < 3
                {
                    attempt.follow()
                } else {
                    attempt.stop()
                }
            }))
            .build()
            .expect("the HTTP client builds");
        Self { client }
    }

    /// The same client, for posting to a mediator.
    pub fn http(&self) -> &reqwest::Client {
        &self.client
    }

    /// Where `did` is written to: the first `DIDCommMessaging` endpoint of its
    /// document that is a URL. A mediator publishes one; that is what this is
    /// for.
    pub async fn endpoint(&self, did: &str) -> DidcommResult<Option<String>> {
        let Some(doc) = self.resolve(did).await? else {
            return Ok(None);
        };
        Ok(doc.service.iter().find_map(|s| match &s.service_endpoint {
            ServiceKind::DIDCommMessaging { value } if !value.uri.starts_with("did:") => {
                Some(value.uri.clone())
            }
            _ => None,
        }))
    }

    async fn web(&self, did: &str) -> DidcommResult<Option<DIDDoc>> {
        let url = web_url(did)?;
        let response =
            self.client.get(&url).send().await.map_err(|e| {
                Error::msg(ErrorKind::IoError, format!("did:web fetch failed: {e}"))
            })?;
        if response.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(None);
        }
        if !response.status().is_success() {
            return Err(Error::msg(
                ErrorKind::DIDNotResolved,
                format!("did:web fetch answered {}", response.status()),
            ));
        }
        let json: Value = response
            .json()
            .await
            .map_err(|e| Error::msg(ErrorKind::Malformed, format!("did:web document: {e}")))?;
        parse(did, &json).map(Some)
    }
}

impl Default for Resolver {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl DIDResolver for Resolver {
    async fn resolve(&self, did: &str) -> DidcommResult<Option<DIDDoc>> {
        if did.starts_with("did:key:") {
            return key(did).map(Some);
        }
        if did.starts_with("did:web:") {
            return self.web(did).await;
        }
        Err(Error::msg(
            ErrorKind::Unsupported,
            "only did:key and did:web are resolved",
        ))
    }
}

/// The document a `did:key` is: a function of the key in the identifier. An
/// Ed25519 key gets its X25519 key-agreement key derived the way the method
/// specifies, which is what makes a pairwise reachable by DIDComm with nothing
/// published.
pub fn key(did: &str) -> DidcommResult<DIDDoc> {
    let encoded = did
        .strip_prefix("did:key:")
        .filter(|rest| rest.starts_with('z'))
        .ok_or_else(|| Error::msg(ErrorKind::Malformed, "not a base58btc did:key"))?;
    let bytes = bs58::decode(&encoded[1..])
        .into_vec()
        .map_err(|_| Error::msg(ErrorKind::Malformed, "did:key is not base58btc"))?;
    let (codec, raw) = bytes.split_at(bytes.len().min(2));
    let key: [u8; 32] = raw
        .try_into()
        .map_err(|_| Error::msg(ErrorKind::Malformed, "did:key does not hold a 32-byte key"))?;

    let mut verification_method = Vec::new();
    let mut authentication = Vec::new();
    let mut key_agreement = Vec::new();

    if codec == ED25519_PUB {
        let verifying = VerifyingKey::from_bytes(&key).map_err(|_| {
            Error::msg(ErrorKind::Malformed, "did:key holds an invalid Ed25519 key")
        })?;
        let signing_id = format!("{did}#{encoded}");
        verification_method.push(method(
            &signing_id,
            did,
            VerificationMethodType::Ed25519VerificationKey2020,
            encoded.to_owned(),
        ));
        authentication.push(signing_id);

        let x25519 = multibase(&X25519_PUB, verifying.to_montgomery().as_bytes());
        let agreement_id = format!("{did}#{x25519}");
        verification_method.push(method(
            &agreement_id,
            did,
            VerificationMethodType::X25519KeyAgreementKey2020,
            x25519,
        ));
        key_agreement.push(agreement_id);
    } else if codec == X25519_PUB {
        let agreement_id = format!("{did}#{encoded}");
        verification_method.push(method(
            &agreement_id,
            did,
            VerificationMethodType::X25519KeyAgreementKey2020,
            encoded.to_owned(),
        ));
        key_agreement.push(agreement_id);
    } else {
        return Err(Error::msg(
            ErrorKind::Unsupported,
            "only Ed25519 and X25519 did:key identifiers are resolved",
        ));
    }

    Ok(DIDDoc {
        id: did.to_owned(),
        key_agreement,
        authentication,
        verification_method,
        // Nothing: a did:key cannot say where it is reached. The mediator's
        // address is what a relationship keeps instead.
        service: vec![],
    })
}

fn method(
    id: &str,
    controller: &str,
    type_: VerificationMethodType,
    public_key_multibase: String,
) -> VerificationMethod {
    VerificationMethod {
        id: id.to_owned(),
        type_,
        controller: controller.to_owned(),
        verification_material: VerificationMaterial::Multibase {
            public_key_multibase,
        },
    }
}

/// The URL a `did:web` is read from. `did:web:host` reads
/// `https://host/.well-known/did.json`; `did:web:host:a:b` reads
/// `https://host/a/b/did.json`; a port is percent-encoded in the identifier.
/// Only a loopback host is read over plain HTTP, so that a mediator on a
/// developer's machine can be reached; the method requires HTTPS everywhere.
pub fn web_url(did: &str) -> DidcommResult<String> {
    let rest = did
        .strip_prefix("did:web:")
        .ok_or_else(|| Error::msg(ErrorKind::Malformed, "not a did:web"))?;
    let mut parts = rest.split(':');
    let host = parts
        .next()
        .filter(|h| !h.is_empty())
        .ok_or_else(|| Error::msg(ErrorKind::Malformed, "did:web without a host"))?
        .replace("%3A", ":")
        .replace("%3a", ":");
    let hostname = host.split(':').next().unwrap_or(&host);
    let scheme = if hostname == "localhost" || hostname == "127.0.0.1" || hostname == "[::1]" {
        "http"
    } else {
        "https"
    };
    let path: Vec<&str> = parts.collect();
    if path
        .iter()
        .any(|segment| segment.is_empty() || segment.contains('/'))
    {
        return Err(Error::msg(
            ErrorKind::Malformed,
            "did:web with an invalid path",
        ));
    }
    Ok(if path.is_empty() {
        format!("{scheme}://{host}/.well-known/did.json")
    } else {
        format!("{scheme}://{host}/{}/did.json", path.join("/"))
    })
}

/// Reads a DID Core document into the library's shape. Verification methods
/// referenced by fragment are made absolute, embedded ones are lifted into the
/// method list, and the first DIDCommMessaging endpoint of each service is the
/// one kept.
fn parse(did: &str, doc: &Value) -> DidcommResult<DIDDoc> {
    if doc["id"].as_str() != Some(did) {
        return Err(Error::msg(
            ErrorKind::Malformed,
            "the document does not describe the DID that was resolved",
        ));
    }

    let mut verification_method: Vec<VerificationMethod> = doc["verificationMethod"]
        .as_array()
        .map(|methods| methods.iter().filter_map(|m| read_method(did, m)).collect())
        .unwrap_or_default();

    let mut relationship = |name: &str| -> Vec<String> {
        doc[name]
            .as_array()
            .map(|entries| {
                entries
                    .iter()
                    .filter_map(|entry| match entry {
                        Value::String(reference) => Some(absolute(did, reference)),
                        embedded @ Value::Object(_) => {
                            let m = read_method(did, embedded)?;
                            let id = m.id.clone();
                            verification_method.push(m);
                            Some(id)
                        }
                        _ => None,
                    })
                    .collect()
            })
            .unwrap_or_default()
    };
    let key_agreement = relationship("keyAgreement");
    let authentication = relationship("authentication");

    let service = doc["service"]
        .as_array()
        .map(|services| {
            services
                .iter()
                .filter_map(|s| read_service(did, s))
                .collect()
        })
        .unwrap_or_default();

    Ok(DIDDoc {
        id: did.to_owned(),
        key_agreement,
        authentication,
        verification_method,
        service,
    })
}

fn absolute(did: &str, reference: &str) -> String {
    if reference.starts_with('#') {
        format!("{did}{reference}")
    } else {
        reference.to_owned()
    }
}

fn read_method(did: &str, value: &Value) -> Option<VerificationMethod> {
    let id = absolute(did, value["id"].as_str()?);
    let type_ = match value["type"].as_str()? {
        "JsonWebKey2020" => VerificationMethodType::JsonWebKey2020,
        "X25519KeyAgreementKey2019" => VerificationMethodType::X25519KeyAgreementKey2019,
        "X25519KeyAgreementKey2020" => VerificationMethodType::X25519KeyAgreementKey2020,
        "Ed25519VerificationKey2018" => VerificationMethodType::Ed25519VerificationKey2018,
        "Ed25519VerificationKey2020" => VerificationMethodType::Ed25519VerificationKey2020,
        "EcdsaSecp256k1VerificationKey2019" => {
            VerificationMethodType::EcdsaSecp256k1VerificationKey2019
        }
        // Multikey carries the same multibase material as the 2020 suites;
        // the library only knows those, and X25519 is what it encrypts to.
        "Multikey" => VerificationMethodType::X25519KeyAgreementKey2020,
        _ => VerificationMethodType::Other,
    };
    let verification_material = if let Some(jwk) = value.get("publicKeyJwk") {
        VerificationMaterial::JWK {
            public_key_jwk: jwk.clone(),
        }
    } else if let Some(multibase) = value["publicKeyMultibase"].as_str() {
        VerificationMaterial::Multibase {
            public_key_multibase: multibase.to_owned(),
        }
    } else if let Some(base58) = value["publicKeyBase58"].as_str() {
        VerificationMaterial::Base58 {
            public_key_base58: base58.to_owned(),
        }
    } else {
        return None;
    };
    Some(VerificationMethod {
        id,
        type_,
        controller: value["controller"].as_str().unwrap_or(did).to_owned(),
        verification_material,
    })
}

fn read_service(did: &str, value: &Value) -> Option<Service> {
    let id = absolute(did, value["id"].as_str()?);
    if value["type"].as_str() != Some("DIDCommMessaging") {
        return Some(Service {
            id,
            service_endpoint: ServiceKind::Other {
                value: value.clone(),
            },
        });
    }
    let endpoint = match &value["serviceEndpoint"] {
        Value::Array(endpoints) => endpoints.first()?.clone(),
        Value::String(uri) => json!({ "uri": uri }),
        object @ Value::Object(_) => object.clone(),
        _ => return None,
    };
    let strings = |name: &str| -> Vec<String> {
        endpoint[name]
            .as_array()
            .map(|items| {
                items
                    .iter()
                    .filter_map(Value::as_str)
                    .map(str::to_owned)
                    .collect()
            })
            .unwrap_or_default()
    };
    Some(Service {
        id,
        service_endpoint: ServiceKind::DIDCommMessaging {
            value: DIDCommMessagingService {
                uri: endpoint["uri"].as_str()?.to_owned(),
                accept: Some(strings("accept")).filter(|a| !a.is_empty()),
                routing_keys: strings("routingKeys"),
            },
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    // The Ed25519 test vector of the did:key method specification, whose
    // derived X25519 key is published alongside it.
    const DID: &str = "did:key:z6MkhaXgBZDvotDkL5257faiztiGiC2QtKLGpbnnEGta2doK";
    const DERIVED: &str = "z6LSj72tK8brWgZja8NLRwPigth2T9QRiG1uH9oKZuKjdh9p";

    #[test]
    fn an_ed25519_key_derives_its_agreement_key() {
        let doc = key(DID).unwrap();
        assert_eq!(doc.authentication, vec![format!("{DID}#{}", &DID[8..])]);
        assert_eq!(doc.key_agreement, vec![format!("{DID}#{DERIVED}")]);
    }

    #[test]
    fn urls_follow_the_method() {
        assert_eq!(
            web_url("did:web:mediator.almena.id").unwrap(),
            "https://mediator.almena.id/.well-known/did.json"
        );
        assert_eq!(
            web_url("did:web:localhost%3A8080").unwrap(),
            "http://localhost:8080/.well-known/did.json"
        );
        assert!(web_url("did:web:almena.id:a/b").is_err());
    }

    #[test]
    fn a_document_names_where_it_is_read() {
        let doc = json!({
            "id": "did:web:mediator.test",
            "verificationMethod": [{
                "id": "#key-2",
                "type": "X25519KeyAgreementKey2020",
                "controller": "did:web:mediator.test",
                "publicKeyMultibase": DERIVED
            }],
            "keyAgreement": ["#key-2"],
            "service": [{
                "id": "#didcomm",
                "type": "DIDCommMessaging",
                "serviceEndpoint": [{ "uri": "https://mediator.test", "accept": ["didcomm/v2"] }]
            }]
        });
        let parsed = parse("did:web:mediator.test", &doc).unwrap();
        assert_eq!(parsed.key_agreement, vec!["did:web:mediator.test#key-2"]);
        let ServiceKind::DIDCommMessaging { value } = &parsed.service[0].service_endpoint else {
            panic!("not a messaging service");
        };
        assert_eq!(value.uri, "https://mediator.test");
    }
}
