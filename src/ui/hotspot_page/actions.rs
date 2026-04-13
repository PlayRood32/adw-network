const BAND_24_GHZ_INDEX: u32 = 0;
const BAND_5_GHZ_INDEX: u32 = 1;
const BAND_AUTO_INDEX: u32 = 2;
const BAND_CUSTOM_INDEX: u32 = 3;

pub(super) fn band_from_selected(selected: u32, custom_band: &str) -> String {
    match selected {
        BAND_24_GHZ_INDEX => "2.4 GHz".to_string(),
        BAND_5_GHZ_INDEX => "5 GHz".to_string(),
        BAND_AUTO_INDEX => "Auto".to_string(),
        _ => custom_band.trim().to_string(),
    }
}

pub(super) fn band_to_selection(band: &str) -> (u32, String) {
    let trimmed = band.trim();
    if trimmed == "2.4 GHz" || trimmed == "2.4 GHz (Wider Range)" {
        return (BAND_24_GHZ_INDEX, String::new());
    }
    if trimmed == "5 GHz" || trimmed == "5 GHz (Faster Speed)" {
        return (BAND_5_GHZ_INDEX, String::new());
    }
    if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("auto") {
        return (BAND_AUTO_INDEX, String::new());
    }

    // * Keep older saved values and vendor-specific bands editable through the custom path.
    (BAND_CUSTOM_INDEX, trimmed.to_string())
}

pub(super) fn is_custom_band_selected(selected: u32) -> bool {
    selected == BAND_CUSTOM_INDEX
}
