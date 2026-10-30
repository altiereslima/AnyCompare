use regex::{NoExpand, Regex, RegexBuilder};
use std::ops::Range;

#[derive(Clone, Default)]
pub struct SearchOptions {
    pub query: String,
    pub case_sensitive: bool,
    pub whole_word: bool,
    pub regular_expression: bool,
}

impl SearchOptions {
    pub fn compile(&self) -> Result<Regex, String> {
        if self.query.is_empty() {
            return Err("Informe o texto a localizar.".into());
        }
        let mut pattern = if self.regular_expression {
            self.query.clone()
        } else {
            regex::escape(&self.query)
        };
        if self.whole_word {
            pattern = format!(r"\b(?:{pattern})\b");
        }
        RegexBuilder::new(&pattern)
            .case_insensitive(!self.case_sensitive)
            .multi_line(true)
            .build()
            .map_err(|error| format!("Expressão inválida: {error}"))
    }

    pub fn ranges(&self, text: &str) -> Result<Vec<Range<usize>>, String> {
        Ok(self.compile()?.find_iter(text).map(|m| m.range()).collect())
    }

    pub fn replace_all(&self, text: &str, replacement: &str) -> Result<(String, usize), String> {
        let expression = self.compile()?;
        let count = expression.find_iter(text).count();
        let result = if self.regular_expression {
            expression.replace_all(text, replacement)
        } else {
            expression.replace_all(text, NoExpand(replacement))
        };
        Ok((result.into_owned(), count))
    }

    pub fn replacement(&self, matched: &str, replacement: &str) -> Result<String, String> {
        let expression = self.compile()?;
        Ok(if self.regular_expression {
            expression.replace(matched, replacement).into_owned()
        } else {
            replacement.to_owned()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unicode_case_words_literal_dollars_and_regex_captures() {
        let mut options = SearchOptions {
            query: "ação".into(),
            ..Default::default()
        };
        let text = "AÇÃO ação ações açãoextra 🦀";
        let ranges = options.ranges(text).unwrap();
        assert_eq!(
            ranges.iter().map(|r| &text[r.clone()]).collect::<Vec<_>>(),
            ["AÇÃO", "ação", "ação"]
        );
        options.whole_word = true;
        assert_eq!(options.ranges(text).unwrap().len(), 2);
        assert_eq!(
            options.replace_all(text, "$1").unwrap().0,
            "$1 $1 ações açãoextra 🦀"
        );
        options.case_sensitive = true;
        assert_eq!(options.ranges(text).unwrap().len(), 1);
        options.query = r"(\p{L}+)-(\d+)".into();
        options.regular_expression = true;
        assert_eq!(
            options.replace_all("ação-12 fim-9", "${2}:${1}").unwrap(),
            ("12:ação 9:fim".into(), 2)
        );
        options.query = "[".into();
        assert!(options.replace_all(text, "X").is_err());
    }
}
