/// The agreed default is Russian, independently of Windows' display language.
/// An English build can be previewed explicitly with --english.
pub fn choose() {
    let language = if std::env::args().skip(1).any(|argument| argument == "--english") { "en" } else { "ru" };
    snowhush_core::text::choose(&[language]);
}
