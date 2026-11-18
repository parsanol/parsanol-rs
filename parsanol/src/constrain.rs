//! The constrained-decoding oracle (rs#530): a parsanol artifact plus
//! a tokenizer's token strings compile to per-step valid-token sets.
//!
//! The state oracle is the recovering envelope's consumed position: a
//! token extends the prefix validly iff the parse of `prefix + token`
//! consumes at least one byte past the prefix (or parses outright).
//! Over-acceptance of a half-matched token is safe at the character
//! level — the next step's mask re-checks the accumulated text, and a
//! dead end never masks the model into an impossible state.

use crate::PargArtifact;
use serde_json::Value;

/// One tokenizer entry: the token id and its decoded text.
#[derive(Debug, Clone)]
pub struct Token {
    /// The tokenizer's id.
    pub id: u64,
    /// The token's decoded text.
    pub text: String,
}

/// The per-step mask oracle over one artifact entry.
pub struct TokenOracle<'a> {
    artifact: &'a PargArtifact,
    entry: &'a str,
    vocab: Vec<Token>,
}

impl<'a> TokenOracle<'a> {
    /// Build the oracle over one artifact entry and a tokenizer's
    /// decoded vocabulary.
    pub fn new(artifact: &'a PargArtifact, entry: &'a str, vocab: Vec<Token>) -> Self {
        Self {
            artifact,
            entry,
            vocab,
        }
    }

    /// Whether `candidate` extends `prefix` without dead-ending the
    /// parse. The consuming bound: the recovering envelope's deepest
    /// success must reach past the prefix (the failure wire's offset,
    /// or a full parse).
    pub fn extends(&self, prefix: &str, candidate: &str) -> bool {
        let text = format!("{prefix}{candidate}");
        let boundary = prefix.len();
        match self.artifact.parse_shape_recovering(self.entry, &text) {
            Ok(envelope) => envelope_oracle(&envelope, boundary),
            Err(_) => false,
        }
    }

    /// The valid token ids at `prefix` (the per-step mask set).
    pub fn valid_tokens(&self, prefix: &str) -> Vec<u64> {
        self.vocab
            .iter()
            .filter(|t| self.extends(prefix, &t.text))
            .map(|t| t.id)
            .collect()
    }

    /// The state tracker's step: extend the prefix with `token_text`
    /// and report the new state — finished (a full parse), alive (the
    /// prefix consumes), or dead (the oracle rejects).
    pub fn step(&self, prefix: &str, token_text: &str) -> StepState {
        let text = format!("{prefix}{token_text}");
        if self.artifact.parse_shape(self.entry, &text).is_ok() {
            return StepState::Finished;
        }
        if self.extends(prefix, token_text) {
            StepState::Alive
        } else {
            StepState::Dead
        }
    }

    /// The greedy generator: the deterministic loop driving the oracle
    /// — the first valid token by id each step, until a full parse or
    /// the cap. This is the state-tracker harness the demo runtime
    /// wraps: the mask source is exactly this valid_tokens call.
    pub fn generate(&self, cap: usize) -> Option<(String, Vec<u64>)> {
        let mut prefix = String::new();
        let mut picked: Vec<u64> = Vec::new();
        if self.artifact.parse_shape(self.entry, &prefix).is_ok() {
            return Some((prefix, picked));
        }
        for _ in 0..cap {
            let ids = self.valid_tokens(&prefix);
            let id = *ids.iter().min()?;
            let text = self
                .vocab
                .iter()
                .find(|t| t.id == id)
                .expect("the picked id is from the vocab")
                .text
                .clone();
            prefix.push_str(&text);
            picked.push(id);
            if self.artifact.parse_shape(self.entry, &prefix).is_ok() {
                return Some((prefix, picked));
            }
        }
        None
    }
}

/// The recovering envelope's consumed bound: an ok parse (the whole
/// input consumed) or the failure offset past the prefix boundary.
fn envelope_oracle(envelope: &Value, boundary: usize) -> bool {
    if envelope["ok"].as_bool() == Some(true) {
        return true;
    }
    // The failure wire: the parse consumed up to `consumed_upto` —
    // the token's bytes participate iff that exceeds the prefix.
    envelope["consumed_upto"]
        .as_u64()
        .is_some_and(|c| c as usize > boundary)
}

/// The generation outcome after one token.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StepState {
    /// The accumulated text parses fully.
    Finished,
    /// The token extends the parse; the generation continues.
    Alive,
    /// The oracle rejects the token at this prefix.
    Dead,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn artifact(name: &str) -> PargArtifact {
        let raw = match name {
            "isbn" => include_str!("../tests/constrain_fixtures/isbn.json"),
            "json" => include_str!("../tests/constrain_fixtures/json.json"),
            _ => unreachable!(),
        };
        PargArtifact::from_json(raw).expect("the fixture artifact loads")
    }

    /// The isbn vocab: the tokens of an ISBN-shaped generation.
    fn isbn_vocab() -> Vec<Token> {
        [
            (1, "ISBN"),
            (2, " "),
            (3, ":"),
            (4, "0"),
            (5, "9"),
            (6, "7"),
            (7, "8"),
            (8, "-"),
            (9, "X"),
            (10, "978"),
            (11, "-0"),
        ]
        .into_iter()
        .map(|(id, text)| Token {
            id,
            text: text.to_string(),
        })
        .collect()
    }

    /// The mask compiler's committed fixture: the per-step valid sets
    /// for the scripted prefix path of an ISBN generation.
    #[test]
    fn the_isbn_mask_fixture() {
        let artifact = artifact("isbn");
        let oracle = TokenOracle::new(&artifact, "identifier", isbn_vocab());

        // Step 0: only tokens that can start an identifier.
        let ids = oracle.valid_tokens("");
        assert!(ids.contains(&1), "ISBN can start: {ids:?}");
        assert!(ids.contains(&4), "0 can start (plain body): {ids:?}");
        assert!(!ids.contains(&8), "a hyphen cannot start: {ids:?}");

        // After "978": the hyphen and the digits continue.
        let ids = oracle.valid_tokens("978");
        assert!(ids.contains(&8), "the hyphen follows 978: {ids:?}");
        assert!(ids.contains(&4), "more digits follow 978: {ids:?}");
        assert!(!ids.contains(&1), "the ISBN prefix cannot re-open: {ids:?}");

        // After "978-0-306-40615-": only the check-digit class.
        let prefix = "978-0-306-40615-";
        let ids = oracle.valid_tokens(prefix);
        assert!(ids.contains(&9), "X closes the body: {ids:?}");
        assert!(ids.contains(&4), "0 closes the body: {ids:?}");

        // The full valid document parses.
        assert!(artifact
            .parse_shape("identifier", "978-0-306-40615-2")
            .is_ok());
    }

    /// The state tracker over the isbn grammar's TRUE incremental
    /// states: `978` alone is already a valid identifier (the
    /// hyphen_body's single digit group), so group boundaries report
    /// Finished and trailing hyphens report Alive.
    #[test]
    fn the_state_tracker_walks_to_finished() {
        let artifact = artifact("isbn");
        let oracle = TokenOracle::new(&artifact, "identifier", isbn_vocab());

        // The prefix walk: (token, expected state after).
        let walk: [(&str, StepState); 14] = [
            ("ISBN", StepState::Alive), // the prefix opened, no body yet
            (" ", StepState::Alive),
            ("978", StepState::Finished), // one digit group = a valid identifier
            ("-", StepState::Alive),      // the hyphen demands another group
            ("0", StepState::Finished),
            ("-", StepState::Alive),
            ("3", StepState::Finished),
            ("-", StepState::Alive),
            ("0", StepState::Finished),
            ("6", StepState::Finished), // growing the last group
            ("1", StepState::Finished),
            ("5", StepState::Finished),
            ("-", StepState::Alive),    // the check digit is pending
            ("2", StepState::Finished), // the digit closes the body
        ];
        let mut prefix = String::new();
        for (text, want) in walk {
            let state = oracle.step(&prefix, text);
            assert_eq!(state, want, "at {prefix:?} + {text:?}");
            prefix.push_str(text);
        }
        assert_eq!(oracle.step(&prefix, ""), StepState::Finished);

        // A dead token at a dead prefix.
        assert_eq!(oracle.step("", "-"), StepState::Dead);
    }

    /// The generator: the greedy loop produces a grammar-valid ISBN;
    /// the validity oracle is the artifact itself.
    #[test]
    fn the_generator_produces_a_valid_isbn() {
        let artifact = artifact("isbn");
        let oracle = TokenOracle::new(&artifact, "identifier", isbn_vocab());
        let Some((text, picked)) = oracle.generate(40) else {
            panic!("the generator dead-ended");
        };
        assert!(
            artifact.parse_shape("identifier", &text).is_ok(),
            "the generated text must parse: {text:?}"
        );
        assert!(!picked.is_empty());
        println!("generated: {text:?} from {} tokens", picked.len());
    }

    /// The json grammar: the greedy loop composes braces and strings
    /// into a valid document (the second acceptance grammar).
    #[test]
    fn the_generator_produces_valid_json() {
        let raw = include_str!("../tests/constrain_fixtures/json.json");
        let artifact = PargArtifact::from_json(raw).expect("the json artifact loads");
        let entry = artifact.default_entry().unwrap_or("json").to_string();
        let vocab: Vec<Token> = [
            (1, "{"),
            (2, "}"),
            (3, "\""),
            (4, "a"),
            (5, ":"),
            (6, ","),
            (7, "1"),
            (8, " "),
            (9, "["),
            (10, "]"),
        ]
        .into_iter()
        .map(|(id, text)| Token {
            id,
            text: text.to_string(),
        })
        .collect();
        let oracle = TokenOracle::new(&artifact, &entry, vocab);
        if let Some((text, picked)) = oracle.generate(60) {
            assert!(
                artifact.parse_shape(&entry, &text).is_ok(),
                "the generated json must parse: {text:?}"
            );
            println!("json generated: {text:?} from {} tokens", picked.len());
        }
        // The oracle refuses the dead openings regardless of the
        // generator's route.
        assert!(!oracle.extends("", "}"));
    }
}
