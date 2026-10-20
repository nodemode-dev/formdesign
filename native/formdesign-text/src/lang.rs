const NAMES: &[(&str, &str)] = &[
    ("english", "en"),
    ("english: usa", "en-US"),
    ("english: uk", "en-GB"),
    ("english: canadian", "en-CA"),
    ("german", "de"),
    ("german: 2006 reform", "de"),
    ("german: swiss", "de-CH"),
    ("french", "fr"),
    ("french: canadian", "fr-CA"),
    ("spanish", "es"),
    ("italian", "it"),
    ("portuguese", "pt"),
    ("portuguese: brazilian", "pt-BR"),
    ("dutch", "nl"),
    ("danish", "da"),
    ("swedish", "sv"),
    ("norwegian", "nb"),
    ("norwegian: bokmal", "nb"),
    ("norwegian: nynorsk", "nn"),
    ("finnish", "fi"),
    ("polish", "pl"),
    ("czech", "cs"),
    ("slovak", "sk"),
    ("hungarian", "hu"),
    ("greek", "el"),
    ("russian", "ru"),
    ("ukrainian", "uk"),
    ("turkish", "tr"),
    ("catalan", "ca"),
    ("croatian", "hr"),
    ("slovenian", "sl"),
    ("estonian", "et"),
    ("latvian", "lv"),
    ("lithuanian", "lt"),
    ("romanian", "ro"),
    ("bulgarian", "bg"),
    ("arabic", "ar"),
    ("hebrew", "he"),
    ("japanese", "ja"),
    ("chinese", "zh"),
    ("korean", "ko"),
    ("hindi", "hi"),
    ("no language", ""),
    ("[no language]", ""),
];

pub fn bcp47(name: &str) -> String {
    let n = name.trim().to_ascii_lowercase();
    if let Some((_, code)) = NAMES.iter().find(|(k, _)| *k == n) {
        return code.to_string();
    }
    let looks_like_code = !n.is_empty() && n.len() <= 8 && n.chars().all(|c| c.is_ascii_alphabetic() || c == '-' || c == '_');
    if looks_like_code {
        return n.replace('_', "-");
    }
    "en".into()
}

pub fn primary(name: &str) -> String {
    bcp47(name).split('-').next().unwrap_or("").to_string()
}

pub fn hyphen_lang(name: &str) -> Option<hypher::Lang> {
    let p = primary(name);
    let b = p.as_bytes();
    if b.len() != 2 {
        return None;
    }
    hypher::Lang::from_iso([b[0], b[1]])
}

pub fn is_rtl_lang(name: &str) -> bool {
    matches!(primary(name).as_str(), "ar" | "he" | "fa" | "ur" | "yi")
}
