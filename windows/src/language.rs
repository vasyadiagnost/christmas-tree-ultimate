use windows::Win32::Foundation::{ERROR_SUCCESS, WIN32_ERROR};
use windows::Win32::System::Registry::{HKEY_CURRENT_USER, REG_DWORD, RRF_RT_REG_DWORD, RegGetValueW, RegSetKeyValueW};
use windows::core::w;

use crate::identity;

const VALUE: windows::core::PCWSTR = w!("interfaceLanguage");

/// Stable values stored in the user's application settings.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum Language {
    Russian = 0,
    English = 1,
}

impl Language {
    fn code(self) -> &'static str {
        match self {
            Self::Russian => "ru",
            Self::English => "en",
        }
    }
}

/// A command-line English preview lasts for one launch and never changes the saved selection.
/// Missing, unreadable or unknown settings fall back to Russian, independently of Windows' UI language.
pub fn choose() {
    let mut saved = 0u32;
    let mut size = std::mem::size_of::<u32>() as u32;
    let status =
        unsafe { RegGetValueW(HKEY_CURRENT_USER, identity::SETTINGS, VALUE, RRF_RT_REG_DWORD, None, Some((&raw mut saved).cast()), Some(&mut size)) };
    let english = std::env::args().skip(1).any(|argument| argument == "--english");
    apply(preferred((status == ERROR_SUCCESS).then_some(saved), english));
}

pub fn current() -> Language {
    if snowhush_core::text::code() == "en" { Language::English } else { Language::Russian }
}

/// Persist before changing visible text: failed writes leave the active locale intact.
pub fn select(language: Language) -> Result<(), WIN32_ERROR> {
    let value = language as u32;
    let status = unsafe {
        RegSetKeyValueW(HKEY_CURRENT_USER, identity::SETTINGS, VALUE, REG_DWORD.0, Some((&raw const value).cast()), std::mem::size_of::<u32>() as u32)
    };
    if status != ERROR_SUCCESS {
        return Err(status);
    }
    apply(language);
    Ok(())
}

fn preferred(saved: Option<u32>, english_override: bool) -> Language {
    if english_override || saved == Some(Language::English as u32) { Language::English } else { Language::Russian }
}

fn apply(language: Language) {
    snowhush_core::text::choose(&[language.code()]);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn startup_precedence_and_invalid_values_keep_the_agreed_default() {
        for saved in [None, Some(0), Some(2), Some(u32::MAX)] {
            assert_eq!(preferred(saved, false), Language::Russian);
            assert_eq!(preferred(saved, true), Language::English);
        }
        assert_eq!(preferred(Some(1), false), Language::English);
        assert_eq!(preferred(Some(1), true), Language::English);
    }
}
