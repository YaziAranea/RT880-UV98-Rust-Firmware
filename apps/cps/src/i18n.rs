//! CPS presentation language. Never translate codeplug contents or protocol data.
use std::borrow::Cow;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum Language {
    #[default]
    Russian,
    English,
    German,
}

impl Language {
    pub const ALL: [Self; 3] = [Self::Russian, Self::English, Self::German];
    pub const fn index(self) -> usize {
        match self {
            Self::Russian => 0,
            Self::English => 1,
            Self::German => 2,
        }
    }
    pub const fn code(self) -> &'static str {
        match self {
            Self::Russian => "ru",
            Self::English => "en",
            Self::German => "de",
        }
    }
    pub const fn autonym(self) -> &'static str {
        match self {
            Self::Russian => "Русский",
            Self::English => "English",
            Self::German => "Deutsch",
        }
    }
    pub fn from_code(code: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|language| language.code() == code.trim())
    }
}

/// Retains all three variants so existing notices and worker results can change
/// language without repeating the operation or touching the document.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct LocalizedText([Cow<'static, str>; 3]);

impl LocalizedText {
    pub const fn borrowed(ru: &'static str, en: &'static str, de: &'static str) -> Self {
        Self([Cow::Borrowed(ru), Cow::Borrowed(en), Cow::Borrowed(de)])
    }
    pub fn from_fn(mut render: impl FnMut(Language) -> String) -> Self {
        Self(Language::ALL.map(|language| Cow::Owned(render(language))))
    }
    pub fn as_str(&self, language: Language) -> &str {
        &self.0[language.index()]
    }
    pub fn is_empty(&self) -> bool {
        self.0.iter().all(|text| text.is_empty())
    }
}

// Use only for language-neutral data: paths, OS errors, raw technical details.
impl From<String> for LocalizedText {
    fn from(text: String) -> Self {
        Self([
            Cow::Owned(text.clone()),
            Cow::Owned(text.clone()),
            Cow::Owned(text),
        ])
    }
}
impl From<&str> for LocalizedText {
    fn from(text: &str) -> Self {
        text.to_owned().into()
    }
}

#[macro_export]
macro_rules! tr {
    ($language:expr, $ru:literal, $en:literal, $de:literal $(,)?) => {
        match $language {
            $crate::i18n::Language::Russian => $ru,
            $crate::i18n::Language::English => $en,
            $crate::i18n::Language::German => $de,
        }
    };
}

#[macro_export]
macro_rules! trf {
    ($language:expr, $ru:literal, $en:literal, $de:literal $(, $($args:tt)*)?) => {
        match $language {
            $crate::i18n::Language::Russian => format!($ru $(, $($args)*)?),
            $crate::i18n::Language::English => format!($en $(, $($args)*)?),
            $crate::i18n::Language::German => format!($de $(, $($args)*)?),
        }
    };
}

#[macro_export]
macro_rules! msg {
    ($ru:literal, $en:literal, $de:literal $(,)?) => {
        $crate::i18n::LocalizedText::borrowed($ru, $en, $de)
    };
}

#[macro_export]
macro_rules! msgf {
    ($ru:literal, $en:literal, $de:literal $(, $($args:tt)*)?) => {
        $crate::i18n::LocalizedText::from_fn(|language| $crate::trf!(language, $ru, $en, $de $(, $($args)*)?))
    };
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn stored_notice_changes_language_without_recreating_it() {
        let count = 7;
        let notice = msgf!("Каналов: {count}", "Channels: {count}", "Kanäle: {count}");
        assert_eq!(notice.as_str(Language::Russian), "Каналов: 7");
        assert_eq!(notice.as_str(Language::English), "Channels: 7");
        assert_eq!(notice.as_str(Language::German), "Kanäle: 7");
        assert_eq!(Language::from_code(" de\n"), Some(Language::German));
        assert_eq!(Language::from_code("invalid"), None);
    }
}
