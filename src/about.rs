//! The About dialog, filled from the metainfo (bundled in the resources), with its Details page
//! turned into Support, as in Notav and Todav.

use adw::prelude::*;
use gtk::glib;

/// Where to support development, listed in About (and in the README), as in Notav and Todav.
const DONATIONS: [(&str, &str); 3] = [
    ("Support on Ko-fi", "https://ko-fi.com/N5E229GJPZ"),
    ("Sponsor on GitHub", "https://github.com/sponsors/SelfRef"),
    (
        "Donate on Liberapay",
        "https://liberapay.com/SelfRef/donate",
    ),
];

pub fn present(parent: &impl IsA<gtk::Widget>) {
    let d = adw::AboutDialog::from_appdata(
        "/dev/aperte/Gextant/metainfo.xml",
        Some(env!("CARGO_PKG_VERSION")),
    );
    d.set_copyright("© 2026 SelfRef");
    support(&d);
    d.present(Some(parent));
}

/// Turns About's Details page into Support: a note and the donation links instead of the
/// description, with the website left on the main page. AdwAboutDialog has no API for that, so
/// this finds its rows (the same in libadwaita 1.8 to 1.10); if they move, About keeps a Details
/// page with the note and the links.
fn support(d: &adw::AboutDialog) {
    d.set_comments(&glib::markup_escape_text(
        "Gextant is developed in my free time. If it keeps your extensions fresh, you can \
         support its development.",
    ));
    for (title, uri) in DONATIONS {
        d.add_link(title, uri);
    }
    // Its content: the dialog itself has no children until presented.
    let Some(content) = d.child() else { return };
    let widgets = descendants(&content);
    // The row that opens the page, followed by the website's own (hidden with a Details page).
    let Some(row) = widgets
        .iter()
        .filter_map(|w| w.downcast_ref::<adw::ActionRow>())
        .find(|r| {
            r.action_name().as_deref() == Some("navigation.push")
                && r.action_target_value()
                    .is_some_and(|v| v.str() == Some("details"))
        })
    else {
        return;
    };
    let Some(page) = widgets
        .iter()
        .find_map(|w| w.downcast_ref::<adw::NavigationView>())
        .and_then(|v| v.find_page("details"))
    else {
        return;
    };
    row.set_title("_Support");
    page.set_title("Support");
    if let Some(website) = row.next_sibling() {
        website.set_visible(true);
    }
    for r in descendants(page.upcast_ref()) {
        if let Some(r) = r.downcast_ref::<adw::PreferencesRow>()
            && !DONATIONS.iter().any(|(t, _)| *t == r.title())
        {
            r.set_visible(false);
        }
    }
}

/// `w` and every widget inside it.
fn descendants(w: &gtk::Widget) -> Vec<gtk::Widget> {
    let mut all = vec![w.clone()];
    let mut i = 0;
    while let Some(w) = all.get(i).cloned() {
        let mut child = w.first_child();
        while let Some(c) = child {
            child = c.next_sibling();
            all.push(c);
        }
        i += 1;
    }
    all
}
