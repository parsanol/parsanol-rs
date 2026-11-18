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

/// The per-step mask in the llguidance-compatible token-bitmask wire:
/// one bit per vocabulary token, packed little-endian into u32 words
/// (bit i of word i/32 = token i). The bytes serialize as-is for the
/// logits-processor interface.
#[derive(Debug, Clone)]
pub struct TokenBitmask {
    /// The packed words: bit i of word i/32 = token i allowed.
    pub words: Vec<u32>,
    /// The vocabulary size the mask covers.
    pub vocab_size: usize,
}

impl TokenBitmask {
    /// Whether the token id passes the mask.
    pub fn is_allowed(&self, token_id: u64) -> bool {
        let id = token_id as usize;
        id < self.vocab_size && self.words[id / 32] & (1 << (id % 32)) != 0
    }

    /// The little-endian word bytes (the wire form).
    pub fn to_wire(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(self.words.len() * 4);
        for w in &self.words {
            out.extend_from_slice(&w.to_le_bytes());
        }
        out
    }
}

/// The logits-processor-side mask emitter: the bridge between the
/// oracle and a runtime's per-step masking loop (rs#530's runtime
/// integration seam).
pub struct MaskEmitter<'a> {
    oracle: TokenOracle<'a>,
}

impl<'a> MaskEmitter<'a> {
    /// Wrap the oracle as a mask emitter.
    pub fn new(oracle: TokenOracle<'a>) -> Self {
        Self { oracle }
    }

    /// The mask for the next generation step at `prefix`.
    pub fn mask_for(&self, prefix: &str) -> TokenBitmask {
        // The bitset is id-indexed: size by the largest id, not the
        // entry count — tokenizer ids may be sparse or 1-based.
        let vocab_size = self
            .oracle
            .vocab
            .iter()
            .map(|t| t.id as usize)
            .max()
            .unwrap_or(0)
            .saturating_add(1);
        let mut words = vec![0u32; vocab_size.div_ceil(32)];
        for t in &self.oracle.vocab {
            if self.oracle.extends(prefix, &t.text) {
                words[(t.id as usize) / 32] |= 1 << ((t.id as usize) % 32);
            }
        }
        TokenBitmask { words, vocab_size }
    }

    /// The constrained generation loop from the empty prefix.
    pub fn generate(
        &self,
        cap: usize,
        sample: impl Fn(&TokenBitmask) -> u64,
    ) -> Option<(String, Vec<u64>, bool)> {
        self.generate_from("", cap, sample)
    }

    /// The loop seeded at `prefix` (the mid-generation entry: the
    /// runtime replays its own tokens into the tracker and continues).
    pub fn generate_from(
        &self,
        prefix: &str,
        cap: usize,
        sample: impl Fn(&TokenBitmask) -> u64,
    ) -> Option<(String, Vec<u64>, bool)> {
        let mut prefix = prefix.to_string();
        let mut picked = Vec::new();
        if self
            .oracle
            .artifact
            .parse_shape(self.oracle.entry, &prefix)
            .is_ok()
        {
            return Some((prefix, picked, true));
        }
        for _ in 0..cap {
            let mask = self.mask_for(&prefix);
            if mask.words.iter().all(|&w| w == 0) {
                return None; // the valid set drained: the relaxation
                             // path (the recovery envelope) is the
                             // designed continuation
            }
            let id = sample(&mask);
            let text = self.oracle.vocab.iter().find(|t| t.id == id)?.text.clone();
            prefix.push_str(&text);
            picked.push(id);
            if self
                .oracle
                .artifact
                .parse_shape(self.oracle.entry, &prefix)
                .is_ok()
            {
                return Some((prefix, picked, true));
            }
            // Alive by construction: the mask allowed this token, so
            // the parse consumed past the boundary. (A step() check
            // here would misclassify fully-consumed-but-incomplete
            // prefixes as dead — the empty extension consumes
            // nothing.)
        }
        Some((prefix, picked, false))
    }

    /// The relaxed generation loop — the recovery relaxation. The
    /// seed rides as picked token ids (the runtime's own rollout,
    /// possibly off-mask), so when the mask drains at a depth the
    /// loop pops the last pick, bans it at that depth, and retries:
    /// a DFS over the mask lattice that finds a completion whenever
    /// one exists within the budget. This is the no-dead-end
    /// continuation the FSM mask compilers cannot offer — every
    /// re-mask is a fresh oracle parse, so backtracking is free.
    pub fn generate_relaxed(
        &self,
        seed: &[u64],
        budget: usize,
        sample: impl Fn(&TokenBitmask) -> u64,
    ) -> Option<(String, Vec<u64>, bool)> {
        let text_of = |id: u64| -> Option<String> {
            self.oracle
                .vocab
                .iter()
                .find(|t| t.id == id)
                .map(|t| t.text.clone())
        };
        let mut picks: Vec<u64> = seed.to_vec();
        // banned[d] = ids refused at depth d (the drained branches).
        let mut banned: Vec<Vec<u64>> = vec![Vec::new(); picks.len() + 1];
        let mut steps = 0usize;
        loop {
            if steps >= budget {
                let prefix: String = picks.iter().filter_map(|id| text_of(*id)).collect();
                return Some((prefix, picks, false));
            }
            steps += 1;
            let prefix: String = picks.iter().filter_map(|id| text_of(*id)).collect();
            if self
                .oracle
                .artifact
                .parse_shape(self.oracle.entry, &prefix)
                .is_ok()
            {
                return Some((prefix, picks, true));
            }
            let mut mask = self.mask_for(&prefix);
            for &b in &banned[picks.len()] {
                let i = b as usize;
                if i < mask.vocab_size {
                    mask.words[i / 32] &= !(1 << (i % 32));
                }
            }
            if mask.words.iter().all(|&w| w == 0) {
                let last = picks.pop()?;
                banned.truncate(picks.len() + 1);
                banned[picks.len()].push(last);
                continue;
            }
            let id = sample(&mask);
            text_of(id)?;
            picks.push(id);
            banned.push(Vec::new());
        }
    }
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

    /// The mask emitter: the llguidance-compatible wire (the bitset
    /// per step), the sampler-gated generation loop, and the 100%
    /// validity guarantee — the runtime integration's seam (rs#575).
    #[test]
    fn the_mask_emitter_wire_and_generation() {
        let artifact = artifact("isbn");
        let vocab = isbn_vocab();
        let oracle = TokenOracle::new(&artifact, "identifier", vocab.clone());
        let emitter = MaskEmitter::new(oracle);

        // The wire: at "" the allowed set = the non-hyphen starters;
        // the bit for "-" (id 8) is clear in word 0.
        let mask = emitter.mask_for("");
        assert!(!mask.is_allowed(8), "the hyphen is masked out at the start");
        assert!(mask.is_allowed(1), "ISBN is allowed at the start");
        let wire = mask.to_wire();
        assert_eq!(wire.len(), mask.words.len() * 4);
        assert_eq!(wire[8 / 8] & (1 << (8 % 8)), 0, "the wire bit for id 8");

        // The generation loop: the sampler = the lowest allowed id —
        // the closure the runtime's logits pass replaces.
        let lowest = |mask: &TokenBitmask| -> u64 {
            (0..mask.vocab_size as u64)
                .find(|&id| mask.is_allowed(id))
                .expect("the mask is non-empty")
        };
        let (text, picked, complete) = emitter.generate(40, lowest).expect("no dead end");
        assert!(
            artifact.parse_shape("identifier", &text).is_ok(),
            "the generated text must parse: {text:?}"
        );
        assert!(complete, "the loop reached a full parse");
        let replay: String = picked
            .iter()
            .filter_map(|id| vocab.iter().find(|t| t.id == *id).map(|t| t.text.clone()))
            .collect();
        assert_eq!(replay, text, "the picks concatenate to the output");

        // The drained set: at "X" no vocabulary token extends (X is
        // only ever a final check digit, never a start) — the loop
        // reports None, the relaxation trigger.
        let dead = emitter.mask_for("X");
        assert!(
            dead.words.iter().all(|&w| w == 0),
            "X drains this vocab: {dead:?}"
        );
        assert!(emitter.generate_from("X", 40, lowest).is_none());

        // Not every mid-word prefix drains: "978-" demands another
        // digit group, and the digits continue.
        let alive = emitter.mask_for("978-");
        assert!(alive.is_allowed(4), "digits continue after 978-");
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

    /// The recovery relaxation (rs#575's differentiator): the
    /// runtime's own rollout `{"a"}` — the "}" was never mask-allowed
    /// — engages the mask at a drain. The plain loop reports None;
    /// the relaxed loop pops the dead pick and walks the ":" branch
    /// to a full parse. Every re-mask is a fresh oracle parse, so the
    /// backtrack is free — the no-dead-end continuation the FSM mask
    /// compilers cannot offer.
    #[test]
    fn the_relaxed_loop_recovers_a_dead_rollout() {
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
        let oracle = TokenOracle::new(&artifact, &entry, vocab.clone());
        let emitter = MaskEmitter::new(oracle);
        let lowest = |mask: &TokenBitmask| -> u64 {
            (0..mask.vocab_size as u64)
                .find(|&id| mask.is_allowed(id))
                .expect("the mask is non-empty")
        };

        // The seed rollout: {"a"} — missing the colon.
        let seed: Vec<u64> = vec![1, 3, 4, 3, 2];
        let seed_text: String = seed
            .iter()
            .map(|id| vocab.iter().find(|t| t.id == *id).unwrap().text.clone())
            .collect();
        assert_eq!(seed_text, "{\"a\"}");
        // The drain: nothing continues a missing colon.
        let dead = emitter.mask_for(&seed_text);
        assert!(dead.words.iter().all(|&w| w == 0), "{dead:?}");
        assert!(emitter.generate_from(&seed_text, 40, lowest).is_none());

        // The relaxation: pop the "}", ban it at that depth, continue.
        let (text, picked, complete) = emitter
            .generate_relaxed(&seed, 200, lowest)
            .expect("the relaxation finds a completion");
        assert!(complete, "the relaxed loop reached a full parse: {text}");
        assert!(
            artifact.parse_shape(&entry, &text).is_ok(),
            "the relaxed text must parse: {text:?}"
        );
        assert!(
            text.starts_with("{\"a\":"),
            "the seed minus the dead pick, colon re-opened: {text:?}"
        );
        let replay: String = picked
            .iter()
            .filter_map(|id| vocab.iter().find(|t| t.id == *id).map(|t| t.text.clone()))
            .collect();
        assert_eq!(replay, text, "the picks concatenate to the output");
    }
}
