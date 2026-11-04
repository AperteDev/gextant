//! The GitHub access token, in the Secret Service (the sandbox copy has its own).

use gtk::{gio, glib};
use std::collections::HashMap;

fn schema() -> libsecret::Schema {
    let attrs = HashMap::from([("service", libsecret::SchemaAttributeType::String)]);
    libsecret::Schema::new(&crate::app_id(), libsecret::SchemaFlags::NONE, attrs)
}

const ATTRS: [(&str, &str); 1] = [("service", "github")];

pub fn load() -> Option<String> {
    libsecret::password_lookup_sync(
        Some(&schema()),
        HashMap::from(ATTRS),
        gio::Cancellable::NONE,
    )
    .ok()?
    .map(|s| s.to_string())
}

/// Keep `token`; an empty one removes it.
pub fn store(token: &str) -> Result<(), glib::Error> {
    let attrs = HashMap::from(ATTRS);
    if token.is_empty() {
        return libsecret::password_clear_sync(Some(&schema()), attrs, gio::Cancellable::NONE);
    }
    libsecret::password_store_sync(
        Some(&schema()),
        attrs,
        Some(libsecret::COLLECTION_DEFAULT),
        "Gextant: GitHub access token",
        token,
        gio::Cancellable::NONE,
    )
}
