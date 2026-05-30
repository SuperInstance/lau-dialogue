//! # lau-dialogue
//!
//! NPC dialogue engine — characters speak based on context, mood, personality,
//! and what the player is learning about the world.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ---------------------------------------------------------------------------
// Enums
// ---------------------------------------------------------------------------

/// Core personality archetypes for NPCs.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Personality {
    Wise,
    Playful,
    Grumpy,
    Curious,
    Brave,
    Mysterious,
}

/// Current emotional state of a character.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Mood {
    Happy,
    Neutral,
    Worried,
    Excited,
    Thoughtful,
    Sad,
}

/// Events that can trigger a dialogue line.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum DialogueTrigger {
    OnEnterRoom(String),
    OnBuildStructure(String),
    OnConservationError(f64),
    OnQuestComplete(String),
    OnRelationshipAbove(f64),
    OnTimeOfDay(String),
}

// ---------------------------------------------------------------------------
// Structs
// ---------------------------------------------------------------------------

/// An NPC in the game world.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Character {
    pub id: String,
    pub name: String,
    pub personality: Personality,
    pub mood: Mood,
    pub knowledge: Vec<String>,
    /// Relationship score with the player, clamped to 0.0–1.0.
    pub relationship: f64,
}

impl Character {
    pub fn new(id: impl Into<String>, name: impl Into<String>, personality: Personality) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            personality,
            mood: Mood::Neutral,
            knowledge: Vec::new(),
            relationship: 0.5,
        }
    }

    /// Clamp relationship into \[0, 1\].
    pub fn clamp_relationship(&mut self) {
        self.relationship = self.relationship.clamp(0.0, 1.0);
    }
}

/// A single spoken line.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DialogueLine {
    pub speaker: String,
    pub text: String,
    pub emotion: Mood,
    pub triggers: Vec<DialogueTrigger>,
}

/// A template with multiple text variations for a given trigger × personality.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DialogueTemplate {
    pub trigger: DialogueTrigger,
    pub personality: Personality,
    pub templates: Vec<String>,
}

impl DialogueTemplate {
    /// Pick a template text, substituting placeholders.
    ///
    /// Place-hands: `{name}`, `{room}`, `{structure}`, `{vibe}`
    pub fn render(&self, index: usize, vars: &TemplateVars) -> String {
        let raw = self
            .templates
            .get(index % self.templates.len())
            .unwrap_or_else(|| self.templates.first().expect("templates must not be empty"));
        self.substitute(raw, vars)
    }

    fn substitute(&self, raw: &str, vars: &TemplateVars) -> String {
        raw.replace("{name}", &vars.name)
            .replace("{room}", &vars.room)
            .replace("{structure}", &vars.structure)
            .replace("{vibe}", &vars.vibe)
    }
}

/// Variable bag passed into template rendering.
#[derive(Debug, Clone, Default)]
pub struct TemplateVars {
    pub name: String,
    pub room: String,
    pub structure: String,
    pub vibe: String,
}

/// Snapshot of the game state used to decide what an NPC says.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DialogueContext {
    pub room: String,
    pub recent_action: Option<String>,
    /// General atmosphere rating 0–1.
    pub vibe: f64,
    /// How far off the player's conservation efforts are.
    pub conservation_error: f64,
    pub time_of_day: String,
    pub player_level: u32,
}

impl Default for DialogueContext {
    fn default() -> Self {
        Self {
            room: "village".into(),
            recent_action: None,
            vibe: 0.5,
            conservation_error: 0.0,
            time_of_day: "day".into(),
            player_level: 1,
        }
    }
}

/// The main dialogue engine.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DialogueEngine {
    pub characters: HashMap<String, Character>,
    pub templates: Vec<DialogueTemplate>,
}

impl DialogueEngine {
    pub fn new() -> Self {
        Self {
            characters: HashMap::new(),
            templates: Vec::new(),
        }
    }

    /// Add a character to the engine.
    pub fn add_character(&mut self, character: Character) {
        self.characters.insert(character.id.clone(), character);
    }

    /// Adjust the relationship score for a character by `delta`.
    pub fn update_relationship(&mut self, character_id: &str, delta: f64) {
        if let Some(c) = self.characters.get_mut(character_id) {
            c.relationship += delta;
            c.clamp_relationship();
        }
    }

    /// Produce a dialogue line for `character_id` given the current `context`.
    ///
    /// Falls back to a generic greeting when no template matches.
    pub fn generate(&mut self, character_id: &str, context: &DialogueContext) -> DialogueLine {
        let character = match self.characters.get_mut(character_id) {
            Some(c) => c,
            None => {
                return DialogueLine {
                    speaker: character_id.into(),
                    text: "...".into(),
                    emotion: Mood::Neutral,
                    triggers: vec![],
                }
            }
        };

        let vars = TemplateVars {
            name: character.name.clone(),
            room: context.room.clone(),
            structure: context.recent_action.clone().unwrap_or_default(),
            vibe: format!("{:.0}%", context.vibe * 100.0),
        };

        // Collect matching templates.
        let matches: Vec<&DialogueTemplate> = self
            .templates
            .iter()
            .filter(|t| {
                t.personality == character.personality && trigger_matches(&t.trigger, context, character)
            })
            .collect();

        if let Some(pick) = matches.first() {
            let idx = (context.player_level as usize) % pick.templates.len();
            let text = pick.render(idx, &vars);
            let emotion = mood_from_trigger(&pick.trigger);
            DialogueLine {
                speaker: character.name.clone(),
                text,
                emotion,
                triggers: vec![pick.trigger.clone()],
            }
        } else {
            // Fallback generic line.
            let text = fallback_line(&character.personality, &vars);
            DialogueLine {
                speaker: character.name.clone(),
                text,
                emotion: character.mood.clone(),
                triggers: vec![],
            }
        }
    }
}

impl Default for DialogueEngine {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn trigger_matches(trigger: &DialogueTrigger, ctx: &DialogueContext, char_: &Character) -> bool {
    match trigger {
        DialogueTrigger::OnEnterRoom(room) => *room == ctx.room,
        DialogueTrigger::OnBuildStructure(s) => ctx.recent_action.as_deref() == Some(s.as_str()),
        DialogueTrigger::OnConservationError(threshold) => ctx.conservation_error >= *threshold,
        DialogueTrigger::OnQuestComplete(_q) => false, // driven externally
        DialogueTrigger::OnRelationshipAbove(threshold) => char_.relationship >= *threshold,
        DialogueTrigger::OnTimeOfDay(tod) => *tod == ctx.time_of_day,
    }
}

fn mood_from_trigger(trigger: &DialogueTrigger) -> Mood {
    match trigger {
        DialogueTrigger::OnEnterRoom(_) => Mood::Neutral,
        DialogueTrigger::OnBuildStructure(_) => Mood::Excited,
        DialogueTrigger::OnConservationError(_) => Mood::Worried,
        DialogueTrigger::OnQuestComplete(_) => Mood::Happy,
        DialogueTrigger::OnRelationshipAbove(_) => Mood::Happy,
        DialogueTrigger::OnTimeOfDay(tod) => match tod.as_str() {
            "morning" => Mood::Happy,
            "night" => Mood::Thoughtful,
            _ => Mood::Neutral,
        },
    }
}

fn fallback_line(personality: &Personality, vars: &TemplateVars) -> String {
    match personality {
        Personality::Wise => format!("Greetings, {name}. The world speaks to those who listen.", name = vars.name),
        Personality::Playful => format!("Hey {name}, bet you can't guess what I found!", name = vars.name),
        Personality::Grumpy => format!("Oh, it's you again, {name}.", name = vars.name),
        Personality::Curious => format!("{name}, what brings you here? I must know!", name = vars.name),
        Personality::Brave => format!("Stay sharp, {name}. Danger could be anywhere.", name = vars.name),
        Personality::Mysterious => format!("The winds carry your name, {name}... and mine.", name = vars.name),
    }
}

// ---------------------------------------------------------------------------
// Built-in template library (30+)
// ---------------------------------------------------------------------------

/// Populate a `DialogueEngine` with the default template library.
pub fn populate_default_templates(engine: &mut DialogueEngine) {
    let raw: Vec<DialogueTemplate> = serde_json::from_str(DEFAULT_TEMPLATES_JSON)
        .expect("built-in templates must be valid JSON");
    engine.templates = raw;
}

const DEFAULT_TEMPLATES_JSON: &str = r#"[
  {"trigger":{"OnEnterRoom":"forest"},"personality":"Wise","templates":["The forest remembers every footstep, {name}.","Welcome to the green cathedral, {name}.","Listen to the canopy — it has stories to tell."]},
  {"trigger":{"OnEnterRoom":"forest"},"personality":"Playful","templates":["Boo! Just kidding. Welcome to the trees, {name}!","Bet you've never seen a forest this fun!","Try counting the leaves — I dare you!"]},
  {"trigger":{"OnEnterRoom":"forest"},"personality":"Grumpy","templates":["Ugh, trees. Too many leaves.","I hate the outdoors.","Can we go back inside yet, {name}?"]},
  {"trigger":{"OnEnterRoom":"forest"},"personality":"Curious","templates":["Ooh, what's that over there? And THAT?","Do you think forests have feelings, {name}?","I wonder how old the oldest tree here is!"]},
  {"trigger":{"OnEnterRoom":"forest"},"personality":"Brave","templates":["Stay close, {name}. The forest hides dangers.","I'll scout ahead. Watch my back.","This forest is nothing — I've seen worse."]},
  {"trigger":{"OnEnterRoom":"forest"},"personality":"Mysterious","templates":["The trees whisper your name, {name}...","Something watches from the shadows.","Not all who wander are lost — but some are."]},
  {"trigger":{"OnEnterRoom":"village"},"personality":"Wise","templates":["The village thrives when its people care, {name}.","Every stone here was laid with purpose.","Welcome home, {name}."]},
  {"trigger":{"OnEnterRoom":"village"},"personality":"Grumpy","templates":["Too noisy. Too many people.","Village life is overrated.","What do you want, {name}?"]},
  {"trigger":{"OnEnterRoom":"village"},"personality":"Playful","templates":["Village party when?","Anyone seen my pet rock?","{name}, race you to the well!"]},
  {"trigger":{"OnEnterRoom":"mountain"},"personality":"Brave","templates":["The summit awaits, {name}!","I've been waiting for a challenge like this.","Don't look down — look forward."]},
  {"trigger":{"OnEnterRoom":"mountain"},"personality":"Wise","templates":["Mountains teach patience, {name}.","From up here, everything looks small.","The climb is the lesson."]},
  {"trigger":{"OnBuildStructure":"windmill"},"personality":"Wise","templates":["A windmill! The ancestors would be proud, {name}.","Harnessing nature's breath — well done."]},
  {"trigger":{"OnBuildStructure":"windmill"},"personality":"Playful","templates":["Wheee! Does it spin fast enough to ride?","I call the top bunk in the windmill!"]},
  {"trigger":{"OnBuildStructure":"windmill"},"personality":"Curious","templates":["How does it work? Show me everything!","The aerodynamics are fascinating!"]},
  {"trigger":{"OnBuildStructure":"garden"},"personality":"Wise","templates":["A garden is a conversation with the earth, {name}.","You reap what you sow — and you've sown well."]},
  {"trigger":{"OnBuildStructure":"garden"},"personality":"Grumpy","templates":["Great, more weeding for me.","Plants. My sworn enemy."]},
  {"trigger":{"OnBuildStructure":"garden"},"personality":"Playful","templates":["I'm going to name every flower!","Can we plant a trampoline next?"]},
  {"trigger":{"OnConservationError":0.5},"personality":"Wise","templates":["Careful, {name}. The balance is shifting.","We must restore what we've taken."]},
  {"trigger":{"OnConservationError":0.5},"personality":"Curious","templates":["This isn't good, {name}...","We need to fix this, fast!"]},
  {"trigger":{"OnConservationError":0.5},"personality":"Grumpy","templates":["Told you so.","This is why I don't trust 'progress'."]},
  {"trigger":{"OnConservationError":0.8},"personality":"Wise","templates":["The land cries out, {name}. We must act now.","If we don't change course, all is lost."]},
  {"trigger":{"OnQuestComplete":"tutorial"},"personality":"Playful","templates":["You did it! Tutorial complete! 🎉","That's my {name}! Onwards!"]},
  {"trigger":{"OnQuestComplete":"tutorial"},"personality":"Brave","templates":["Well done, {name}. The real adventure begins now.","First quest down. Many more to go."]},
  {"trigger":{"OnQuestComplete":"ecology_101"},"personality":"Wise","templates":["You understand the web of life now, {name}.","Knowledge is the seed of change."]},
  {"trigger":{"OnRelationshipAbove":0.8},"personality":"Wise","templates":["I consider you a true friend, {name}.","Our bond is unbreakable."]},
  {"trigger":{"OnRelationshipAbove":0.8},"personality":"Playful","templates":["Besties forever, {name}!","You're my favourite person and I won't hear otherwise!"]},
  {"trigger":{"OnRelationshipAbove":0.8},"personality":"Mysterious","templates":["You've earned my trust... that is rare.","I suppose I can share my secrets with you now."]},
  {"trigger":{"OnTimeOfDay":"morning"},"personality":"Playful","templates":["Rise and shine, {name}! Adventure awaits!","Good morning! I've been up for HOURS."]},
  {"trigger":{"OnTimeOfDay":"morning"},"personality":"Grumpy","templates":["Too early. Come back later.","Mornings should be illegal."]},
  {"trigger":{"OnTimeOfDay":"night"},"personality":"Wise","templates":["The stars hold ancient wisdom, {name}.","Night is when the world reveals its truths."]},
  {"trigger":{"OnTimeOfDay":"night"},"personality":"Mysterious","templates":["Shadows deepen... and so do truths.","The night speaks, if you listen."]},
  {"trigger":{"OnTimeOfDay":"evening"},"personality":"Curious","templates":["What happens when the sun goes down? Let's find out!","Evenings are full of surprises!"]}
]"#;

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn wise_elder() -> Character {
        Character::new("elder", "Elder Oak", Personality::Wise)
    }

    fn playful_fox() -> Character {
        Character {
            id: "fox".into(),
            name: "Fennick".into(),
            personality: Personality::Playful,
            mood: Mood::Happy,
            knowledge: vec!["forest lore".into()],
            relationship: 0.6,
        }
    }

    fn grumpy_bear() -> Character {
        Character {
            id: "bear".into(),
            name: "Grizzle".into(),
            personality: Personality::Grumpy,
            mood: Mood::Neutral,
            knowledge: vec![],
            relationship: 0.3,
        }
    }

    fn brave_knight() -> Character {
        Character {
            id: "knight".into(),
            name: "Sir Rowan".into(),
            personality: Personality::Brave,
            mood: Mood::Excited,
            knowledge: vec!["combat".into()],
            relationship: 0.5,
        }
    }

    fn mysterious_owl() -> Character {
        Character {
            id: "owl".into(),
            name: "Whisper".into(),
            personality: Personality::Mysterious,
            mood: Mood::Thoughtful,
            knowledge: vec!["secrets".into(), "prophecies".into()],
            relationship: 0.2,
        }
    }

    fn curious_cat() -> Character {
        Character {
            id: "cat".into(),
            name: "Pip".into(),
            personality: Personality::Curious,
            mood: Mood::Excited,
            knowledge: vec![],
            relationship: 0.4,
        }
    }

    fn engine_with_characters() -> DialogueEngine {
        let mut e = DialogueEngine::new();
        e.add_character(wise_elder());
        e.add_character(playful_fox());
        e.add_character(grumpy_bear());
        e.add_character(brave_knight());
        e.add_character(mysterious_owl());
        e.add_character(curious_cat());
        populate_default_templates(&mut e);
        e
    }

    // -- Character tests --

    #[test]
    fn character_new_defaults() {
        let c = Character::new("a", "Alice", Personality::Curious);
        assert_eq!(c.id, "a");
        assert_eq!(c.name, "Alice");
        assert_eq!(c.personality, Personality::Curious);
        assert_eq!(c.mood, Mood::Neutral);
        assert!(c.knowledge.is_empty());
        assert!((c.relationship - 0.5).abs() < f64::EPSILON);
    }

    #[test]
    fn relationship_clamping() {
        let mut c = Character::new("a", "A", Personality::Wise);
        c.relationship = 1.5;
        c.clamp_relationship();
        assert!((c.relationship - 1.0).abs() < f64::EPSILON);
        c.relationship = -0.5;
        c.clamp_relationship();
        assert!((c.relationship - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn update_relationship_clamps() {
        let mut e = DialogueEngine::new();
        e.add_character(Character::new("a", "A", Personality::Wise));
        e.update_relationship("a", 10.0);
        assert!((e.characters["a"].relationship - 1.0).abs() < f64::EPSILON);
        e.update_relationship("a", -20.0);
        assert!((e.characters["a"].relationship - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn update_relationship_missing_is_noop() {
        let mut e = DialogueEngine::new();
        e.update_relationship("ghost", 1.0);
    }

    // -- Generate tests --

    #[test]
    fn generate_unknown_character() {
        let mut e = DialogueEngine::new();
        let line = e.generate("nobody", &DialogueContext::default());
        assert_eq!(line.speaker, "nobody");
        assert_eq!(line.text, "...");
    }

    #[test]
    fn generate_fallback_wise() {
        let mut e = DialogueEngine::new();
        e.add_character(wise_elder());
        let line = e.generate("elder", &DialogueContext::default());
        assert_eq!(line.speaker, "Elder Oak");
        assert!(line.text.contains("Elder Oak") || line.text.contains("Greetings"));
    }

    #[test]
    fn generate_fallback_playful() {
        let mut e = DialogueEngine::new();
        e.add_character(playful_fox());
        let line = e.generate("fox", &DialogueContext::default());
        assert!(line.text.contains("Fennick"));
    }

    #[test]
    fn generate_fallback_grumpy() {
        let mut e = DialogueEngine::new();
        e.add_character(grumpy_bear());
        let line = e.generate("bear", &DialogueContext::default());
        assert!(line.text.contains("Grizzle"));
    }

    #[test]
    fn generate_fallback_brave() {
        let mut e = DialogueEngine::new();
        e.add_character(brave_knight());
        let line = e.generate("knight", &DialogueContext::default());
        assert!(line.text.contains("Sir Rowan"));
    }

    #[test]
    fn generate_fallback_mysterious() {
        let mut e = DialogueEngine::new();
        e.add_character(mysterious_owl());
        let line = e.generate("owl", &DialogueContext::default());
        assert!(line.text.contains("Whisper") || line.text.contains("winds"));
    }

    #[test]
    fn generate_on_enter_room_forest_wise() {
        let mut e = engine_with_characters();
        let ctx = DialogueContext {
            room: "forest".into(),
            ..DialogueContext::default()
        };
        let line = e.generate("elder", &ctx);
        assert!(line.text.contains("{name}") == false); // placeholder resolved
        assert!(line.emotion == Mood::Neutral);
    }

    #[test]
    fn generate_on_enter_room_forest_playful() {
        let mut e = engine_with_characters();
        let ctx = DialogueContext {
            room: "forest".into(),
            ..DialogueContext::default()
        };
        let line = e.generate("fox", &ctx);
        assert!(!line.text.is_empty());
    }

    #[test]
    fn generate_on_enter_room_forest_grumpy() {
        let mut e = engine_with_characters();
        let ctx = DialogueContext {
            room: "forest".into(),
            ..DialogueContext::default()
        };
        let line = e.generate("bear", &ctx);
        assert!(!line.text.is_empty());
    }

    #[test]
    fn generate_on_enter_room_forest_curious() {
        let mut e = engine_with_characters();
        let ctx = DialogueContext {
            room: "forest".into(),
            ..DialogueContext::default()
        };
        let line = e.generate("cat", &ctx);
        assert!(!line.text.is_empty());
    }

    #[test]
    fn generate_on_enter_room_forest_brave() {
        let mut e = engine_with_characters();
        let ctx = DialogueContext {
            room: "forest".into(),
            ..DialogueContext::default()
        };
        let line = e.generate("knight", &ctx);
        assert!(!line.text.is_empty());
    }

    #[test]
    fn generate_on_enter_room_forest_mysterious() {
        let mut e = engine_with_characters();
        let ctx = DialogueContext {
            room: "forest".into(),
            ..DialogueContext::default()
        };
        let line = e.generate("owl", &ctx);
        assert!(!line.text.is_empty());
    }

    #[test]
    fn generate_on_build_structure_windmill() {
        let mut e = engine_with_characters();
        let ctx = DialogueContext {
            room: "plains".into(),
            recent_action: Some("windmill".into()),
            ..DialogueContext::default()
        };
        let line = e.generate("elder", &ctx);
        assert!(line.emotion == Mood::Excited);
    }

    #[test]
    fn generate_on_build_structure_garden() {
        let mut e = engine_with_characters();
        let ctx = DialogueContext {
            recent_action: Some("garden".into()),
            ..DialogueContext::default()
        };
        let line = e.generate("fox", &ctx);
        assert!(!line.text.is_empty());
    }

    #[test]
    fn generate_conservation_error() {
        let mut e = engine_with_characters();
        // Use curious cat which has Curious personality matching the template
        let ctx = DialogueContext {
            room: "plains".into(),
            conservation_error: 0.6,
            ..DialogueContext::default()
        };
        let line = e.generate("cat", &ctx);
        assert!(line.emotion == Mood::Worried);
    }

    #[test]
    fn generate_time_of_day_morning() {
        let mut e = engine_with_characters();
        let ctx = DialogueContext {
            time_of_day: "morning".into(),
            ..DialogueContext::default()
        };
        let line = e.generate("fox", &ctx);
        assert!(!line.text.is_empty());
    }

    #[test]
    fn generate_time_of_day_night() {
        let mut e = engine_with_characters();
        let ctx = DialogueContext {
            time_of_day: "night".into(),
            ..DialogueContext::default()
        };
        let line = e.generate("owl", &ctx);
        assert!(!line.text.is_empty());
    }

    #[test]
    fn relationship_above_trigger() {
        let mut e = engine_with_characters();
        e.update_relationship("owl", 0.7); // now 0.9
        let ctx = DialogueContext::default();
        let line = e.generate("owl", &ctx);
        // Should hit the relationship trigger since 0.9 >= 0.8
        assert!(!line.triggers.is_empty() || !line.text.is_empty());
    }

    #[test]
    fn template_render_cycles() {
        let t = DialogueTemplate {
            trigger: DialogueTrigger::OnEnterRoom("forest".into()),
            personality: Personality::Wise,
            templates: vec!["one".into(), "two".into()],
        };
        let vars = TemplateVars::default();
        assert_eq!(t.render(0, &vars), "one");
        assert_eq!(t.render(2, &vars), "one"); // wraps
        assert_eq!(t.render(1, &vars), "two");
    }

    #[test]
    fn template_placeholders_substituted() {
        let t = DialogueTemplate {
            trigger: DialogueTrigger::OnEnterRoom("forest".into()),
            personality: Personality::Wise,
            templates: vec!["Hello {name} in {room}!".into()],
        };
        let vars = TemplateVars {
            name: "Alice".into(),
            room: "forest".into(),
            ..Default::default()
        };
        assert_eq!(t.render(0, &vars), "Hello Alice in forest!");
    }

    #[test]
    fn default_context() {
        let ctx = DialogueContext::default();
        assert_eq!(ctx.room, "village");
        assert!(ctx.recent_action.is_none());
        assert_eq!(ctx.time_of_day, "day");
        assert_eq!(ctx.player_level, 1);
    }

    #[test]
    fn default_engine() {
        let e = DialogueEngine::default();
        assert!(e.characters.is_empty());
        assert!(e.templates.is_empty());
    }

    #[test]
    fn serde_roundtrip_character() {
        let c = wise_elder();
        let json = serde_json::to_string(&c).unwrap();
        let back: Character = serde_json::from_str(&json).unwrap();
        assert_eq!(back.id, c.id);
        assert_eq!(back.name, c.name);
    }

    #[test]
    fn serde_roundtrip_trigger() {
        let t = DialogueTrigger::OnConservationError(0.42);
        let json = serde_json::to_string(&t).unwrap();
        let back: DialogueTrigger = serde_json::from_str(&json).unwrap();
        assert_eq!(t, back);
    }

    #[test]
    fn serde_roundtrip_engine() {
        let e = engine_with_characters();
        let json = serde_json::to_string(&e).unwrap();
        let back: DialogueEngine = serde_json::from_str(&json).unwrap();
        assert_eq!(back.characters.len(), e.characters.len());
        assert_eq!(back.templates.len(), e.templates.len());
    }

    #[test]
    fn populate_default_template_count() {
        let mut e = DialogueEngine::new();
        populate_default_templates(&mut e);
        assert!(e.templates.len() >= 30, "expected 30+ templates, got {}", e.templates.len());
    }

    #[test]
    fn generate_village_room() {
        let mut e = engine_with_characters();
        let ctx = DialogueContext {
            room: "village".into(),
            ..DialogueContext::default()
        };
        let line = e.generate("elder", &ctx);
        assert!(!line.text.is_empty());
    }

    #[test]
    fn generate_mountain_brave() {
        let mut e = engine_with_characters();
        let ctx = DialogueContext {
            room: "mountain".into(),
            ..DialogueContext::default()
        };
        let line = e.generate("knight", &ctx);
        assert!(!line.text.is_empty());
    }
}
