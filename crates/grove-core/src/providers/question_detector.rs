/// A question detected from raw CLI output via heuristic pattern matching.
#[derive(Debug, Clone)]
pub struct DetectedQuestion {
    pub question: String,
    pub options: Vec<String>,
    pub confidence: f32,
}

/// Attempt to detect a question from a single line of CLI output.
///
/// Returns `Some(DetectedQuestion)` if the line looks like an interactive
/// prompt waiting for user input. Returns `None` for normal output lines.
pub fn detect_question(line: &str) -> Option<DetectedQuestion> {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return None;
    }

    // High confidence: explicit binary choice markers
    if let Some(q) = detect_binary_choice(trimmed) {
        return Some(q);
    }

    // High confidence: numbered choice markers like [1/2/3]
    if let Some(q) = detect_numbered_choice(trimmed) {
        return Some(q);
    }

    // High confidence: common CLI prompts
    if let Some(q) = detect_common_prompt(trimmed) {
        return Some(q);
    }

    // Medium confidence: line ending with ? (useful with stall detection later)
    if trimmed.ends_with('?') && trimmed.len() > 5 {
        return Some(DetectedQuestion {
            question: trimmed.to_string(),
            options: vec![],
            confidence: 0.6,
        });
    }

    // Medium confidence: input prompts like "Enter X:" or "Type X:"
    if let Some(q) = detect_input_prompt(trimmed) {
        return Some(q);
    }

    None
}

fn detect_binary_choice(line: &str) -> Option<DetectedQuestion> {
    let lower = line.to_lowercase();
    let patterns = ["[y/n]", "[yes/no]", "(y/n)", "(yes/no)"];
    for pat in &patterns {
        if lower.contains(pat) {
            return Some(DetectedQuestion {
                question: line.to_string(),
                options: vec!["Yes".to_string(), "No".to_string()],
                confidence: 0.95,
            });
        }
    }
    None
}

fn detect_numbered_choice(line: &str) -> Option<DetectedQuestion> {
    // Match patterns like [1/2/3] or (1/2/3) — manual parsing, no regex dependency
    for (open, close) in [('[', ']'), ('(', ')')] {
        if let Some(start) = line.rfind(open) {
            if let Some(end) = line[start..].find(close) {
                let inner = &line[start + 1..start + end];
                let parts: Vec<&str> = inner.split('/').collect();
                if parts.len() >= 2
                    && parts
                        .iter()
                        .all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_digit()))
                {
                    return Some(DetectedQuestion {
                        question: line.to_string(),
                        options: parts.iter().map(|p| p.to_string()).collect(),
                        confidence: 0.9,
                    });
                }
            }
        }
    }
    None
}

fn detect_common_prompt(line: &str) -> Option<DetectedQuestion> {
    let lower = line.to_lowercase();
    let prompts = [
        "continue?",
        "proceed?",
        "overwrite?",
        "replace?",
        "delete?",
        "press enter",
        "hit enter",
        "hit any key",
        "press any key",
    ];
    for p in &prompts {
        if lower.contains(p) {
            return Some(DetectedQuestion {
                question: line.to_string(),
                options: vec![],
                confidence: 0.9,
            });
        }
    }
    None
}

fn detect_input_prompt(line: &str) -> Option<DetectedQuestion> {
    let lower = line.to_lowercase();
    if (lower.starts_with("enter ") || lower.starts_with("type ") || lower.starts_with("input "))
        && lower.ends_with(':')
    {
        return Some(DetectedQuestion {
            question: line.to_string(),
            options: vec![],
            confidence: 0.7,
        });
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_yn_prompt() {
        let q = detect_question("Continue? [y/n]").unwrap();
        assert!(q.confidence > 0.9);
        assert_eq!(q.options, vec!["Yes", "No"]);
    }

    #[test]
    fn detects_yes_no_prompt() {
        let q = detect_question("Overwrite existing file? (yes/no)").unwrap();
        assert!(q.confidence > 0.9);
    }

    #[test]
    fn detects_numbered_choice() {
        let q = detect_question("Select an option [1/2/3]").unwrap();
        assert!(q.confidence > 0.85);
        assert_eq!(q.options, vec!["1", "2", "3"]);
    }

    #[test]
    fn detects_common_prompts() {
        assert!(detect_question("Press enter to continue").is_some());
        assert!(detect_question("Proceed?").is_some());
    }

    #[test]
    fn detects_question_mark() {
        let q = detect_question("Do you want to install the package?").unwrap();
        assert!(q.confidence >= 0.5 && q.confidence <= 0.7);
    }

    #[test]
    fn detects_input_prompt() {
        let q = detect_question("Enter your API key:").unwrap();
        assert!(q.confidence >= 0.6);
    }

    #[test]
    fn ignores_normal_output() {
        assert!(detect_question("Compiling grove-core v0.1.0").is_none());
        assert!(detect_question("  Finished dev [unoptimized] target").is_none());
        assert!(detect_question("").is_none());
    }
}
