//! The access tokens (GitHub, GitLab), in the Secret Service (the sandbox copy has its own).

use gtk::{gio, glib};
use std::collections::HashMap;

fn schema() -> libsecret::Schema {
    let attrs = HashMap::from([("service", libsecret::SchemaAttributeType::String)]);
    libsecret::Schema::new(&crate::app_id(), libsecret::SchemaFlags::NONE, attrs)
}

/// `service` is `github` or `gitlab`.
pub fn load(service: &str) -> Option<String> {
    let attrs = HashMap::from([("service", service)]);
    libsecret::password_lookup_sync(Some(&schema()), attrs, gio::Cancellable::NONE)
        .ok()?
        .map(|s| s.to_string())
}

/// Keep `token` for `service`; an empty one removes it.
pub fn store(service: &str, token: &str) -> Result<(), glib::Error> {
    let attrs = HashMap::from([("service", service)]);
    if token.is_empty() {
        return libsecret::password_clear_sync(Some(&schema()), attrs, gio::Cancellable::NONE);
    }
    libsecret::password_store_sync(
        Some(&schema()),
        attrs,
        Some(libsecret::COLLECTION_DEFAULT),
        &format!("Gextant: {service} access token"),
        token,
        gio::Cancellable::NONE,
    )
}

/// What was kept, for a run of checks.
#[derive(Clone, Default)]
pub struct Tokens {
    pub github: Option<String>,
    pub gitlab: Option<String>,
}

impl Tokens {
    pub fn load() -> Self {
        Tokens {
            github: load("github"),
            gitlab: load("gitlab"),
        }
    }

    /// The token for requests to `host`. A gitlab.com token is for gitlab.com only: another
    /// GitLab host (gitlab.gnome.org) is somebody else's server, which never gets it.
    pub fn for_host(&self, host: &str) -> Option<&str> {
        match host {
            "github.com" => self.github.as_deref(),
            "gitlab.com" => self.gitlab.as_deref(),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_token_goes_only_to_its_own_host() {
        let t = Tokens {
            github: Some("gh".into()),
            gitlab: Some("gl".into()),
        };
        assert_eq!(t.for_host("github.com"), Some("gh"));
        assert_eq!(t.for_host("gitlab.com"), Some("gl"));
        assert_eq!(t.for_host("gitlab.gnome.org"), None);
        assert_eq!(t.for_host("evil.example"), None);
        assert_eq!(Tokens::default().for_host("gitlab.com"), None);
    }
}
