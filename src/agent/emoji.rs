use regex::Regex;

/// Comprehensive emoji removal layer for Manager communications.
/// Ensures ZERO emojis reach the Manager in text or audio transcripts.
pub fn strip_emojis(input: &str) -> String {
    // Regex matching standard Unicode Emoji ranges, emoticons, symbols, and pictographs
    let emoji_pattern = r"(?x)
        [\u{1F600}-\u{1F64F}] | # Emoticons
        [\u{1F300}-\u{1F5FF}] | # Misc Symbols and Pictographs
        [\u{1F680}-\u{1F6FF}] | # Transport and Map Symbols
        [\u{1F700}-\u{1F77F}] | # Alchemical Symbols
        [\u{1F780}-\u{1F7FF}] | # Geometric Shapes Extended
        [\u{1F800}-\u{1F8FF}] | # Supplemental Arrows-C
        [\u{1F900}-\u{1F9FF}] | # Supplemental Symbols and Pictographs
        [\u{1FA00}-\u{1FA6F}] | # Chess Symbols
        [\u{1FA70}-\u{1FAFF}] | # Symbols and Pictographs Extended-A
        [\u{2600}-\u{26FF}]   | # Misc Symbols
        [\u{2700}-\u{27BF}]   | # Dingbats
        [\u{FE00}-\u{FE0F}]   | # Variation Selectors
        [\u{1F1E6}-\u{1F1FF}]   # Regional Indicator Symbols (Flags)
    ";

    let re = Regex::new(emoji_pattern).unwrap();
    let cleaned = re.replace_all(input, "");

    // Remove spaces before punctuation left over by removed emojis (e.g. "اليوم :" -> "اليوم:")
    let punct_space_re = Regex::new(r" +([:,.!?؛،])").unwrap();
    let cleaned = punct_space_re.replace_all(&cleaned, "$1");
    
    // Collapse multiple consecutive blank spaces
    let space_re = Regex::new(r" {2,}").unwrap();
    let result = space_re.replace_all(&cleaned, " ");

    result.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_strip_emojis() {
        let sample = "تقرير اليوم 👍: تم إنجاز العمل بنسبة 100% 🚀🔥 ممتازة جداً 😊";
        let cleaned = strip_emojis(sample);
        assert_eq!(cleaned, "تقرير اليوم: تم إنجاز العمل بنسبة 100% ممتازة جداً");
    }
}
