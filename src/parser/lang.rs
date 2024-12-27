pub type Lang = &'static str;

pub const LANG_ZH: Lang = "zh";
pub const LANG_JP: Lang = "jp";
pub const LANG_EN: Lang = "en";
pub const LANG_ZH_CN: Lang = "zh-CN";
pub const LANG_ZH_TW: Lang = "zh-TW";
pub const LANG_UNKNOWN: Lang = "";

pub(super) fn from(lang: lingua::Language) -> Lang {
    match lang {
        lingua::Language::Chinese => LANG_ZH,
        lingua::Language::English => LANG_EN,
        lingua::Language::Japanese => LANG_JP,
    }
}
