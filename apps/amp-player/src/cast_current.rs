//! Current playback is derived from active output, never library selection.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Current {
    pub title: String,
    pub supported: bool,
}
impl Current {
    pub fn resolve(decks: [Option<String>; 2], sampler: bool, bypass: bool) -> Self {
        if bypass {
            return Self { title: "Radio, auxiliary deck or preview output is active; this output cannot be cast yet".into(), supported: false };
        }
        let mut titles: Vec<String> = decks.into_iter().flatten().collect();
        if sampler {
            titles.push("Sampler".into());
        }
        if titles.is_empty() {
            Self {
                title: "No active Player / DJ master playback".into(),
                supported: false,
            }
        } else {
            Self {
                title: format!("Player / DJ master · {}", titles.join(" + ")),
                supported: true,
            }
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn playing_deck_not_selected_library_row_owns_cast() {
        assert_eq!(
            Current::resolve([Some("Playing".into()), None], false, false).title,
            "Player / DJ master · Playing"
        );
    }
    #[test]
    fn transition_keeps_both_decks_and_sampler_on_one_master() {
        let current = Current::resolve(
            [Some("Outgoing".into()), Some("Incoming".into())],
            true,
            false,
        );
        assert!(current.supported);
        assert!(current.title.contains("Outgoing + Incoming + Sampler"));
    }
    #[test]
    fn bypass_never_silently_casts_a_different_program() {
        assert!(!Current::resolve([Some("Deck".into()), None], false, true).supported);
        assert!(!Current::resolve([None, None], false, false).supported);
        assert!(Current::resolve([None, None], true, false).supported);
    }
}
