//! The wallet's links, in either of their two forms.
//!
//! `almena://…` is the wallet's own scheme, and the only form a computer
//! opens it with. On a phone the same link is also written
//! `https://almena.id/w/…` — whatever follows `almena://`, under `/w/` on the
//! identity domain — which iOS (universal links) and Android (verified App
//! Links) hand only to this app, because `almena.id` names it in
//! `/.well-known/apple-app-site-association` and `assetlinks.json`; any app
//! may claim a custom scheme, nobody else that host. A QR code carries the
//! https form, so a phone's camera opens the wallet too.
//!
//! Both read the same: [`scheme_form`] turns the https one into the
//! `almena://` one, and only for that host and that prefix.

use url::Url;

/// The wallet's own scheme.
pub const SCHEME: &str = "almena";
/// The identity domain the wallet's https links live on, and where under it.
pub const HOST: &str = "almena.id";
pub const PREFIX: &str = "/w/";

/// `input` as an `almena://` link: as it is when it already is one, or the
/// `almena://` link an `https://almena.id/w/…` one stands for. `None` for
/// anything else — another host, a port, credentials, another path, plain
/// HTTP.
pub fn scheme_form(input: &str) -> Option<Url> {
    let link = Url::parse(input.trim()).ok()?;
    if link.scheme() == SCHEME {
        return Some(link);
    }
    if link.scheme() != "https"
        || link.host_str() != Some(HOST)
        || link.port().is_some()
        || !link.username().is_empty()
        || link.password().is_some()
    {
        return None;
    }
    let rest = link.path().strip_prefix(PREFIX)?;
    if rest.is_empty() {
        return None;
    }
    let mut written = format!("{SCHEME}://{rest}");
    if let Some(query) = link.query() {
        written.push('?');
        written.push_str(query);
    }
    if let Some(fragment) = link.fragment() {
        written.push('#');
        written.push_str(fragment);
    }
    Url::parse(&written).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    const QUERY: &str =
        "request_uri=https%3A%2F%2Falmena.id%2Fapi%2Fv1%2Fverifications%2Fx%2Frequest";

    fn form(input: &str) -> Option<String> {
        scheme_form(input).map(String::from)
    }

    #[test]
    fn the_https_link_reads_as_the_scheme_link() {
        let scheme = format!("almena://auth?{QUERY}");
        assert_eq!(
            form(&format!("https://almena.id/w/auth?{QUERY}")),
            Some(scheme.clone())
        );
        assert_eq!(form(&scheme), Some(scheme));
        assert_eq!(
            form("https://almena.id/w/invite?_oob=eyJhIjoiYiJ9"),
            Some("almena://invite?_oob=eyJhIjoiYiJ9".into())
        );
        // A host is a host whatever its case; the system matched it already.
        assert_eq!(
            form("https://ALMENA.ID/w/auth?x=1"),
            Some("almena://auth?x=1".into())
        );
    }

    #[test]
    fn only_that_host_and_prefix() {
        for input in [
            "http://almena.id/w/auth?x=1",
            "https://evil.example/w/auth?x=1",
            "https://almena.id.evil.example/w/auth?x=1",
            "https://sub.almena.id/w/auth?x=1",
            "https://almena.id./w/auth?x=1",
            "https://almena.id:8443/w/auth?x=1",
            "https://user@almena.id/w/auth?x=1",
            "https://almena.id/auth?x=1",
            "https://almena.id/W/auth?x=1",
            "https://almena.id/wallet/auth?x=1",
            "https://almena.id/w%2Fauth?x=1",
            "https://almena.id/x/../auth?x=1",
            "https://almena.id/w/",
            "https://almena.id/w",
            "didcomm://invite?_oob=x",
            "not a link",
            "",
        ] {
            assert_eq!(form(input), None, "{input}");
        }
    }
}
