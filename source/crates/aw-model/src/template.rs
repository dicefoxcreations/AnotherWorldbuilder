//! Templates: what kind of article this is and which fields it has.
//!
//! Character, place, organisation are not special cases in the code, they are
//! data. A template has an infobox of fields and a body of blocks. A block is
//! also a unit of access — each one gets its own rule when a document is
//! made.
//!
//! Everything is a document. The only field kind that picks something is
//! [`FieldKind::Reference`]: a field `species` points at documents of type
//! `species`; the choices are therefore not strings but the documents that
//! exist. Whether those are full articles or bare values (a data type) only
//! changes how they are shown — `male` is a document of type `gender`, and
//! whoever wants to write more about it switches the type to article.
//!
//! A type may have a **supertype**. `City` under `Place` inherits the
//! fields and blocks of `Place`, and a reference to `Place` offers cities
//! too. One reference field per type may be marked `parent`: it is the axis
//! of the tree — “lies in”, “part of”, “belongs to” — from which breadcrumbs
//! and tree views are built.

use serde::{Deserialize, Serialize};

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum TemplateError {
    #[error("a template needs an id")]
    NoSlug,

    #[error("“{0}” is not a valid id: lower-case letters, digits and hyphens")]
    BadSlug(String),

    #[error("field key “{0}” appears more than once")]
    DuplicateField(String),

    #[error("reference field “{0}” names no target type")]
    NoTargetType(String),

    #[error("“{0}” is the parent field and points at exactly one document — it cannot be multiple")]
    MultipleParent(String),

    #[error("a type has at most one parent field — “{0}” and “{1}” both are")]
    TwoParents(String, String),

    #[error("the supertype “{0}” does not exist")]
    UnknownSupertype(String),

    #[error("“{0}” cannot be its own supertype, not even via others")]
    SupertypeCycle(String),

    #[error("“{0}” has width {1} — it does not fit the template")]
    BadSpan(String, u8),

    #[error("“{0}” is nested too deep — three levels at most")]
    TooDeep(String),

    #[error("“{0}” is no property group — a group is made under Options › Types › Groups")]
    NoGroup(String),

    #[error("a property group has properties and a layout — no supertype, no blocks, no infobox")]
    GroupWithMore,

    #[error("the group “{0}” contains itself, via others")]
    GroupCycle(String),

    #[error("a property group cannot be picked — “{0}” is one")]
    PicksGroup(String),

    /// A rule that is no rule — the text says why.
    #[error("{0}")]
    Rule(String),

    /// An element binds a property that does not exist, or one of another
    /// kind — the text says which.
    #[error("{0}")]
    Binding(String),
}

pub fn valid_slug(text: &str) -> bool {
    !text.is_empty()
        && text
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

/// What a field may hold.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum FieldKind {
    /// One line or several, but without formatting.
    Text {
        #[serde(default)]
        multiline: bool,
    },
    /// Flowing text with formatting and wiki links.
    RichText,
    /// A heading inside the article.
    ///
    /// Blocks carry none of their own — whoever wants one places it as a
    /// field. That way it stands where it belongs and not automatically above
    /// everything.
    Heading {
        #[serde(default = "first_level")]
        level: u8,
    },
    Number {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        unit: Option<String>,
    },
    /// A date in the world calendar, stored as a day count — the time of
    /// day, where there is one, as its fraction.
    WorldDate,
    /// A date and time of ours — when a session was played. Stored as
    /// `2026-09-12T19:00`.
    DateTime,
    /// A reference to documents of one type — or of any of its subtypes.
    ///
    /// The choices are the documents of that type. Every entry is therefore a
    /// document itself, and everything stays linked. A data type makes this a
    /// plain selection; an article type makes it a link.
    ///
    /// `parent` marks the axis of the tree. It points at exactly one document;
    /// an empty `article_type` then means “the root of my own type tree” —
    /// a city lies in a place, an order is part of an organisation.
    Reference {
        #[serde(default)]
        article_type: String,
        #[serde(default)]
        multiple: bool,
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        parent: bool,
        /// `adds`: the documents it picks add their numbers to this one's —
        /// a feat with `Strength: 1` raises the character's Strength by one.
        /// Only where it says so: a father's Strength is not the son's.
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        adds: bool,
        /// `with Level`: every pick carries a number of its own, by this
        /// name — the levels in each class, the potions in the bag. Stored
        /// as `{"with": id, "count": 3}`; a formula reads it within the pick
        /// (`Classes.(Level * Hit die)`).
        #[serde(default, skip_serializing_if = "String::is_empty")]
        count: String,
        /// `every`: always every document of the type — each skill, each
        /// saving throw. None is picked or taken out; only their numbers
        /// (`with`) are stored, and only those that are not 0. A document of
        /// the type made later is there too.
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        every: bool,
        /// `from Previous`: nothing is stored — it holds the documents of
        /// the type whose property of that name points here. `Next:
        /// @@Session log@@ from Previous` finds the session after this one.
        #[serde(default, skip_serializing_if = "String::is_empty")]
        from: String,
    },
    /// Relations to documents of a type — each with a role on either side.
    ///
    /// `Relations: @@Character@@ as @@Relation@@`: every entry names a
    /// document and what it is to this one (`Mother`), and what this one is
    /// to it (`Daughter`). The roles are documents of `role_type`, usually a
    /// data type. Always several entries, and always written into both
    /// documents — see `relation`.
    Relation {
        #[serde(default)]
        article_type: String,
        #[serde(default)]
        role_type: String,
    },
    /// The properties of a property group, standing here as if they were
    /// the type's own: `Stats: group Abilities`.
    ///
    /// A group is defined once and used by every type that needs it — the
    /// six abilities by characters, monsters, feats and items alike. What
    /// changes in the group changes everywhere. Its properties keep their
    /// names: `%%Strength%%` reads the same, whether it comes from a group or
    /// not. The group's name only heads them where they are listed.
    Group {
        /// The id of the group — a [`Template`] with `group` set.
        #[serde(default)]
        group: String,
    },
    /// An image, stored by the hash of its content.
    ///
    /// Width and height in pixels, `0` meaning “as wide as there is room”.
    /// A portrait stands tall, a coat of arms is square, a map is wide —
    /// without a size the view would have to guess.
    ///
    /// `align` says where it sits: in the infobox `left`, `center` or
    /// `right`, in flowing text the same — and there the text runs past it
    /// for `left` and `right`. It is the **same** attribute because it is the
    /// same thing: an image with a place.
    Image {
        #[serde(default)]
        width: u16,
        #[serde(default)]
        height: u16,
        #[serde(default = "centered")]
        align: String,
    },
}

impl FieldKind {
    pub fn label(&self) -> &'static str {
        match self {
            FieldKind::Text { multiline: false } => "Text",
            FieldKind::Text { multiline: true } => "Multiline text",
            FieldKind::RichText => "Flowing text",
            FieldKind::Heading { .. } => "Heading",
            FieldKind::Number { .. } => "Number",
            FieldKind::WorldDate => "World date",
            FieldKind::DateTime => "Date and time",
            FieldKind::Reference { parent: true, .. } => "Parent",
            FieldKind::Reference { every: true, .. } => "Every document of a type",
            FieldKind::Reference { multiple: false, .. } => "Reference to a document",
            FieldKind::Reference { multiple: true, .. } => "Several documents",
            FieldKind::Relation { .. } => "Relations",
            FieldKind::Image { .. } => "Image",
            FieldKind::Group { .. } => "Property group",
        }
    }


    /// The type this field points at — if it points at one.
    ///
    /// Empty for a bare `parent`: then the target is the root of the owning
    /// type's tree, see [`crate::types::target_of`].
    pub fn references(&self) -> Option<&str> {
        match self {
            FieldKind::Reference { article_type, .. } | FieldKind::Relation { article_type, .. } => {
                Some(article_type)
            }
            _ => None,
        }
    }

    /// The type the roles of a relation come from.
    pub fn roles(&self) -> Option<&str> {
        match self {
            FieldKind::Relation { role_type, .. } => Some(role_type),
            _ => None,
        }
    }

    /// Whether this field is the axis of the tree.
    pub fn is_parent(&self) -> bool {
        matches!(self, FieldKind::Reference { parent: true, .. })
    }
}

/// One property of a type — a variable every document of it has.
///
/// Properties are data. Where one is shown or changed, an element says so
/// with `bind=` — in the infobox or anywhere in the text; `%%Name%%` reads it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TemplateField {
    /// The id the value is stored under. Generated, never shown.
    pub key: String,
    /// Its name — what documents write before the colon, and what `bind=`
    /// and `%%…%%` call it by.
    pub label: String,
    #[serde(flatten)]
    pub kind: FieldKind,
    /// Access of its own — then the value is a layer of its own, and only
    /// who may see it gets it. `None` takes the article's.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub access: Option<Access>,
    /// What a new document starts with, as written: `alive`,
    /// `@@Species:Human@@`, `42`.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub default: String,
    /// The groups it comes from, outermost first, by the names they are
    /// given — `Stats › Abilities`. Set only where groups are unfolded
    /// ([`crate::types::effective`]), for the lists that show a group's
    /// properties together.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub in_group: Vec<String>,
}

impl TemplateField {
    pub fn new(key: impl Into<String>, label: impl Into<String>, kind: FieldKind) -> Self {
        TemplateField {
            key: key.into(),
            label: label.into(),
            kind,
            access: None,
            default: String::new(),
            in_group: Vec::new(),
        }
    }
}


fn first_level() -> u8 {
    2
}

fn centered() -> String {
    "center".into()
}

/// Where the infobox sits on the page.
///
/// Four alignments cover every wiki page anyone actually wants to see. Free
/// placement was an answer to a question nobody asked — it costs the user
/// work and the application a grid that breaks on flowing text.
///
/// The values are English, like the whole template language: the application
/// is tied neither to one rule set nor to one language.
pub const ALIGNMENTS: [&str; 4] = ["right", "left", "top", "none"];

fn align_right() -> String {
    "right".into()
}

fn width_auto() -> String {
    "auto".into()
}

/// What counts as a width.
///
/// `auto` means: as wide as the content, capped by the view. A number is the
/// width in pixels. `narrow` and `wide` stay as names for the two widths
/// people want most often.
pub fn valid_width(value: &str) -> bool {
    value == "auto" || value == "narrow" || value == "wide" || value.parse::<u16>().is_ok()
}

/// The infobox — a box beside the text that shows properties.
///
/// It holds no data of its own: it lists which properties it shows, in a
/// fixed order, drawn as label-value pairs. No grid: everything in it has a
/// known size and is the same for every article of that type.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Infobox {
    /// One of [`ALIGNMENTS`].
    #[serde(default = "align_right")]
    pub align: String,
    /// `auto`, a width in pixels, or `narrow`/`wide`.
    ///
    /// `auto` is the default: the box is as wide as its content needs. A
    /// fixed width would make every article look the same — but nobody could
    /// say what the number should be, and a box measuring 280 px around a
    /// 200 px image looks like an accident. Anyone who wants it uniform types
    /// a number.
    #[serde(default = "width_auto")]
    pub width: String,
    /// What it shows, in order — `<text bind=Title>` and the like.
    #[serde(default)]
    pub show: Vec<Shown>,
    /// Who sees and changes its values — like a block, `<infobox view=…
    /// edit=…>`. `None`: as the article. A property with access of its own
    /// keeps it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub access: Option<Access>,
}

/// A property shown somewhere: an element with `bind=`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Shown {
    /// The element: `text`, `textarea`, `number`, `date`, `select`, `image`.
    pub element: String,
    /// The property's name.
    pub property: String,
    /// What stands beside it instead of the name — empty for the name.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub label: String,
}

impl Default for Infobox {
    fn default() -> Self {
        Infobox {
            align: align_right(),
            width: width_auto(),
            show: Vec::new(),
            access: None,
        }
    }
}

impl Infobox {
    pub fn visible(&self) -> bool {
        self.align != "none" && !self.show.is_empty()
    }

    /// Brings alignment and width to allowed values.
    ///
    /// Showing nothing, there is no box — then the alignment is `none`.
    /// Otherwise an empty infobox would carry an alignment nobody can see,
    /// and the round trip through the text would come back with another.
    fn normalize(&mut self) {
        if !ALIGNMENTS.contains(&self.align.as_str()) {
            self.align = align_right();
        }
        if self.show.is_empty() {
            self.align = "none".into();
        }
        if !valid_width(&self.width) {
            self.width = width_auto();
        }
    }
}

/// The default when nothing is written: everyone may see and change.
///
/// Deliberately open. A silent restriction is worse than a stated one:
/// whoever wants to protect something writes it down, and whoever does not
/// gets no boundary slipped in that they never knew about.
fn everyone() -> String {
    "all".into()
}

/// The style of a block in the body.
///
/// It steers the look — and **only** the look. Who may see a block stands in
/// its policy and has nothing to do with the style.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum BlockStyle {
    /// No frame — reads as continuous text.
    #[default]
    Text,
    /// Set apart with a frame, for rules and the like.
    Box,
    /// An indented quote.
    Quote,
    /// A creature's or a character's values at a glance, as the classic
    /// stat block: the name, sections, the ability tiles, rolls.
    Statblock,
}

impl BlockStyle {
    pub fn label(&self) -> &'static str {
        match self {
            BlockStyle::Text => "Text",
            BlockStyle::Box => "Box",
            BlockStyle::Quote => "Quote",
            BlockStyle::Statblock => "Stat block",
        }
    }
}

/// A block a new article of this type is given.
///
/// The template suggests blocks; while writing, any number may be added. A
/// block is the unit of access, not of appearance.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TemplateBlock {
    pub key: String,
    /// Heading, written into the text as a `##` line. Empty: none.
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub style: BlockStyle,
    /// The parts a block is made of — at least one.
    ///
    /// A part begins where the permission changes. Why parts at all: a
    /// region someone may not see has to be stored on its own. Were it in the
    /// same layer as the text around it, hiding would be a matter of display —
    /// and whoever may read the block would get it too.
    #[serde(default)]
    pub parts: Vec<BlockPart>,
    /// Where it stands: in the text beside the infobox, above or below
    /// both, across the page's whole width — or in a column at the side.
    #[serde(default)]
    pub place: BlockPlace,
    /// How wide a block at the side stands, in pixels — 0: as wide as the
    /// app finds right.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub width: u16,
    /// Whether its title folds it, and how it starts.
    #[serde(default)]
    pub fold: BlockFold,
    /// How large its title stands: 1 as a `#` heading in the text, down to
    /// 4, the size of the text itself.
    #[serde(default = "first_heading", skip_serializing_if = "is_first_heading")]
    pub heading: u8,
    /// The types a button beside its title makes documents of — `new=`, by
    /// name. Empty: no button.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub new: Vec<String>,
}

/// The heading levels a block title may take.
pub const HEADINGS: std::ops::RangeInclusive<u8> = 1..=4;

fn first_heading() -> u8 {
    1
}

fn is_first_heading(level: &u8) -> bool {
    *level == 1
}

fn is_zero(n: &u16) -> bool {
    *n == 0
}

/// Where a block stands on its page.
///
/// The infobox stands beside the text. What needs the page's width — a
/// character's sheet, a family tree — goes above (`Top`) or below (`Bottom`)
/// both. What one keeps at hand while reading the rest — a timeline — stands
/// in a column of its own at the side (`Side`), and folds away sideways.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BlockPlace {
    #[default]
    Flow,
    Top,
    Bottom,
    Side,
}

/// Whether a block folds: by default its title folds it and it starts
/// open; `Folded` starts with only its title; `Off` does not fold at all.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BlockFold {
    #[default]
    Open,
    Folded,
    Off,
}

/// Who may see and who may change.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Access {
    #[serde(default = "everyone")]
    pub view: String,
    #[serde(default = "everyone")]
    pub edit: String,
}

impl Default for Access {
    fn default() -> Self {
        Access { view: everyone(), edit: everyone() }
    }
}

/// A piece of a block — and at the same time a unit of access.
///
/// When a document is made, each part becomes a part of its own with its own
/// rule. That is why a region with access of its own — an inner
/// `<block view=…>`, an `<image view=…>` — can sit in the middle of flowing
/// text, and who sees it is decided where it is stored.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BlockPart {
    /// Unique within the article — the first part carries the block id,
    /// further ones append a number. That is how the view finds it again.
    pub key: String,
    /// Markdown a new article starts with.
    #[serde(default)]
    pub text: String,
    /// Who may see it.
    #[serde(default = "everyone")]
    pub view: String,
    /// Who may change it.
    #[serde(default = "everyone")]
    pub edit: String,
}

impl Default for BlockPart {
    fn default() -> Self {
        BlockPart {
            key: String::new(),
            text: String::new(),
            view: everyone(),
            edit: everyone(),
        }
    }
}

impl BlockPart {
    pub fn fresh(key: impl Into<String>) -> Self {
        BlockPart {
            key: key.into(),
            ..BlockPart::default()
        }
    }
}

impl TemplateBlock {
    pub fn fresh(key: impl Into<String>, title: impl Into<String>) -> Self {
        let key = key.into();
        TemplateBlock {
            parts: vec![BlockPart::fresh(key.clone())],
            key,
            title: title.into(),
            style: BlockStyle::default(),
            place: BlockPlace::default(),
            fold: BlockFold::default(),
            heading: first_heading(),
            new: Vec::new(),
            width: 0,
        }
    }

    /// The key of the n-th part.
    ///
    /// The first keeps the block id so that existing articles find their
    /// text again.
    pub fn part_key(block: &str, n: usize) -> String {
        if n == 0 {
            block.to_string()
        } else {
            format!("{block}-{}", n + 1)
        }
    }

    /// A block without parts gets one — otherwise an article of this type
    /// would have nowhere to write.
    fn normalize(&mut self) {
        if self.parts.is_empty() {
            self.parts.push(BlockPart::fresh(self.key.clone()));
        }
        for (i, part) in self.parts.iter_mut().enumerate() {
            if part.key.is_empty() {
                part.key = TemplateBlock::part_key(&self.key, i);
            }
        }
    }
}

/// A rule of the game, as a formula: `mod(x) = floor((x - 10) / 2)`,
/// `check(x) = 1d20 + x`, `crit = 20`.
///
/// No game is built in. What a modifier is, what a check rolls, when a roll
/// is a critical — a world says it, in its types and groups. Formulas call a
/// rule as they call `floor`: `mod(Strength)`, or `Strength mod` for a rule
/// of one argument. Reckoned by the app (`shared/Dice.ts`); here it is kept
/// and checked.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rule {
    pub name: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub params: Vec<String>,
    pub formula: String,
}

/// What formulas know without a world saying it — no rule may be called so.
pub const BUILT_IN: [&str; 16] =
    ["floor", "ceil", "round", "abs", "min", "max", "first", "last", "sum", "count", "or", "if", "and", "not", "steps", "map"];

/// A rule's name and each of its arguments: one word.
fn is_word(text: &str) -> bool {
    let mut chars = text.chars();
    chars.next().is_some_and(|c| c.is_alphabetic() || c == '_') && chars.all(|c| c.is_alphanumeric() || c == '_')
}

impl Rule {
    /// Whether it holds together: a word for a name that nothing built in
    /// has, words for arguments, each once, and a formula.
    pub fn validate(&self) -> Result<(), TemplateError> {
        let bad = |why: String| Err(TemplateError::Rule(why));
        if !is_word(&self.name) {
            return bad(format!("“{}” — a rule is called by one word: mod(x) = …", self.name));
        }
        if BUILT_IN.contains(&self.name.to_lowercase().as_str()) {
            return bad(format!("{} is built in — the rule needs another name.", self.name));
        }
        let mut seen = std::collections::BTreeSet::new();
        for p in &self.params {
            if !is_word(p) {
                return bad(format!("{}: “{p}” — an argument is one word: {}(x) = …", self.name, self.name));
            }
            if !seen.insert(p.to_lowercase()) {
                return bad(format!("{} has the argument {p} twice.", self.name));
            }
        }
        if self.formula.trim().is_empty() {
            return bad(format!("{} says nothing after =.", self.name));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Template {
    /// The id. Unchanging, generated, never shown.
    ///
    /// It is what articles and references hang on. It used to be the
    /// lower-cased name — so after a rename the type was called one thing
    /// while its articles sat under another, and every reference had to carry
    /// the old name. Name and id are two things; only apart can one change
    /// without touching the other.
    ///
    pub id: String,
    pub name: String,
    /// An emoji for lists and menus.
    #[serde(default)]
    pub icon: String,
    /// The id of the supertype, empty for a root type.
    ///
    /// The type inherits its supertype's fields and blocks, and everything
    /// that points at the supertype also accepts this type.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub supertype: String,
    /// A property group, not a type: it has properties only, and no
    /// documents. Types take its properties in with a field
    /// [`FieldKind::Group`].
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub group: bool,
    /// Not in the outline the app sets out by itself — a type only other
    /// documents point to. The GM may still place it there.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub navigation_hidden: bool,
    /// The rules of the game its documents reckon with — `mod(x) =
    /// floor((x - 10) / 2)`. Inherited like the properties, and taken in
    /// with a group.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub rules: Vec<Rule>,
    #[serde(default)]
    pub description: String,
    /// The properties every document of this type has — the data.
    #[serde(default)]
    pub properties: Vec<TemplateField>,
    /// The infobox — which of them it shows.
    #[serde(default)]
    pub infobox: Infobox,
    /// A bracket around the whole template.
    ///
    /// Sets the access for everything that says nothing of its own. `None`
    /// means the default: everyone may see and change.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auth: Option<Access>,
    /// How the name of a new article is built.
    ///
    /// `%%input%%` is what the user types; `%%count%%` counts up. A pattern like
    /// `Session log %%count%%` then needs no input at all — and `Item %%input%%`
    /// puts the same word in front of every name without anyone having to
    /// type it each time.
    #[serde(default = "just_input")]
    pub name_pattern: String,
    /// The type's own page — a document with text, images and lists.
    ///
    /// Every type has one, as the world has its home page: the type is not
    /// only a template and a collection but an article of its own. “Places”
    /// can say something about the world and list its continents below. It
    /// comes into being with the type; see `ensure_pages` in the app.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub page: String,
    /// The blocks a new article starts with.
    #[serde(default)]
    pub blocks: Vec<TemplateBlock>,
    /// The order its documents show their blocks in, by key — the inherited
    /// ones among them. Empty: the supertypes' first, then the type's own.
    /// One not named stands after those named.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub order: Vec<String>,
    /// How its documents line up in lists and the outline: by these
    /// properties, by key, one after another — `name` for the name. Empty:
    /// by name.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sort: Vec<String>,
    /// Who besides the GM may make documents of this type — a view, as
    /// `players` or `class:rogue`. Empty: the GM alone. A player's new
    /// document goes to the GM's device, which makes it with this type's
    /// access (see `journal::Proposal`).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub create: String,
}

impl Template {
    /// Checks whether the template holds together on its own.
    ///
    /// A freshly made type has no blocks yet — that is allowed. You create
    /// it and put it together afterwards; anything else would mean it could
    /// not be saved in the first place.
    ///
    /// What depends on other types — the supertype, a parent field inherited
    /// from it — is checked by [`crate::types::validate_hierarchy`].
    pub fn validate(&self) -> Result<(), TemplateError> {
        if self.id.is_empty() {
            return Err(TemplateError::NoSlug);
        }
        if !valid_slug(&self.id) {
            return Err(TemplateError::BadSlug(self.id.clone()));
        }
        if !ALIGNMENTS.contains(&self.infobox.align.as_str()) {
            return Err(TemplateError::BadSpan(self.infobox.align.clone(), 0));
        }

        let mut seen = std::collections::BTreeSet::new();
        let mut parent: Option<&TemplateField> = None;
        let mut names = std::collections::BTreeSet::new();
        for field in &self.properties {
            if !names.insert(field.label.to_lowercase()) {
                return Err(TemplateError::DuplicateField(field.label.clone()));
            }
            if !seen.insert(field.key.clone()) {
                return Err(TemplateError::DuplicateField(field.key.clone()));
            }
            if let FieldKind::Reference {
                article_type,
                multiple,
                parent: is_parent,
                ..
            } = &field.kind
            {
                if *is_parent {
                    if *multiple {
                        return Err(TemplateError::MultipleParent(field.label.clone()));
                    }
                    if let Some(first) = parent {
                        return Err(TemplateError::TwoParents(
                            first.label.clone(),
                            field.label.clone(),
                        ));
                    }
                    parent = Some(field);
                } else if article_type.is_empty() {
                    return Err(TemplateError::NoTargetType(field.key.clone()));
                }
            }
            if let FieldKind::Relation { article_type, role_type } = &field.kind {
                if article_type.is_empty() || role_type.is_empty() {
                    return Err(TemplateError::NoTargetType(field.key.clone()));
                }
            }
        }

        let mut rules = std::collections::BTreeSet::new();
        for rule in &self.rules {
            rule.validate()?;
            if !rules.insert(rule.name.to_lowercase()) {
                return Err(TemplateError::Rule(format!("{} has the rule {} twice.", self.name, rule.name)));
            }
        }

        let mut keys = std::collections::BTreeSet::new();
        for block in &self.blocks {
            if !keys.insert(block.key.clone()) {
                return Err(TemplateError::DuplicateField(block.key.clone()));
            }
        }
        Ok(())
    }

    /// Brings the template into shape.
    ///
    /// Fills in what is missing — the id of a new type, the parts of a block.
    pub fn normalize(&mut self) {
        // A type without an id is new.
        if self.id.is_empty() {
            self.id = crate::template_text::new_id();
        }
        for block in &mut self.blocks {
            block.normalize();
        }
        self.infobox.normalize();
    }

    /// The template's own properties. Inherited ones come from
    /// [`crate::types::effective`].
    pub fn fields(&self) -> Vec<&TemplateField> {
        self.properties.iter().collect()
    }

    pub fn field(&self, key: &str) -> Option<&TemplateField> {
        self.properties.iter().find(|f| f.key == key)
    }

    /// A property by its name — as text is written — or by its key, as
    /// text is stored.
    pub fn property(&self, name: &str) -> Option<&TemplateField> {
        let name = name.trim();
        self.properties.iter().find(|f| f.key == name || f.label.eq_ignore_ascii_case(name))
    }


}


// --- Name pattern -------------------------------------------------------

/// What may stand in a name pattern.
pub const PATTERN_INPUT: &str = "%%input%%";
/// Counts the articles of this type up.
pub const PATTERN_COUNT: &str = "%%count%%";

/// The default: the name is exactly what is typed.
pub fn just_input() -> String {
    PATTERN_INPUT.into()
}

/// Checks a name pattern and says what is wrong with it.
///
/// Two things can go wrong: a placeholder nobody knows (usually a typo), and
/// a pattern with no placeholder at all — then every article of the type
/// would have the same name.
pub fn check_pattern(pattern: &str) -> Result<(), String> {
    let mut rest = pattern;
    let mut found = 0;
    while let Some(opening) = rest.find("%%") {
        let after = &rest[opening + 2..];
        let Some(close) = after.find("%%") else {
            return Err(format!(
                "The name pattern is missing a closing %% — you probably meant {PATTERN_INPUT}."
            ));
        };
        let whole = format!("%%{}%%", &after[..close]);
        if whole != PATTERN_INPUT && whole != PATTERN_COUNT {
            return Err(format!("Nobody knows {whole}. There is {PATTERN_INPUT} and {PATTERN_COUNT}."));
        }
        found += 1;
        rest = &after[close + 2..];
    }
    if found == 0 {
        return Err(format!(
            "The name pattern needs {PATTERN_INPUT} or {PATTERN_COUNT} — otherwise every article of this type would have the same name."
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_type_kept_out_of_the_outline_says_so_and_only_then() {
        let shown: Template = serde_json::from_str(r#"{"id":"book","name":"Book"}"#).unwrap();
        assert!(!shown.navigation_hidden);
        let hidden: Template = serde_json::from_str(r#"{"id":"gender","name":"Gender","navigation_hidden":true}"#).unwrap();
        assert!(hidden.navigation_hidden);
        assert_eq!(serde_json::to_value(hidden).unwrap()["navigation_hidden"], true);
        assert!(serde_json::to_value(shown).unwrap().get("navigation_hidden").is_none());
    }

    /// A template that contains everything there is.
    ///
    /// A world starts empty, there are no templates shipped with it. For the
    /// tests a fixture of our own is more precise anyway: it holds every
    /// field kind, including those nobody would create on their own.
    fn probe() -> Template {
        let mut t = Template {
            id: "probe".into(),
            name: "Probe".into(),
            supertype: String::new(),
            group: false,
            navigation_hidden: false,
            rules: Vec::new(),
            icon: "🧪".into(),
            description: "For the tests".into(),
            auth: None,
            name_pattern: just_input(),
            order: Vec::new(),
            sort: Vec::new(),
            create: String::new(),
            page: String::new(),
            properties: vec![
                    TemplateField::new("a", "Text", FieldKind::Text { multiline: false }),
                    TemplateField::new("b", "Paragraph", FieldKind::Text { multiline: true }),
                    TemplateField::new(
                        "c",
                        "Number",
                        FieldKind::Number {
                            unit: Some("years".into()),
                        },
                    ),
                    TemplateField::new("d", "Date", FieldKind::WorldDate),
                    TemplateField::new(
                        "e",
                        "Lies in",
                        FieldKind::Reference {
                            article_type: String::new(),
                            multiple: false,
                            parent: true,
                            adds: false,
                            count: String::new(),
                            every: false,
                            from: String::new(),
                        },
                    ),
                    TemplateField::new(
                        "g",
                        "Reference",
                        FieldKind::Reference {
                            article_type: "probe".into(),
                            multiple: true,
                            parent: false,
                            adds: true,
                            count: String::new(),
                            every: false,
                            from: String::new(),
                        },
                    ),
                    TemplateField::new(
                        "h",
                        "",
                        FieldKind::Image {
                            width: 200,
                            height: 260,
                            align: "center".into(),
                        },
                    ),
                ],
            infobox: Infobox {
                align: "right".into(),
                ..Infobox::default()
            },
            blocks: vec![
                TemplateBlock::fresh("free".to_string(), "Open".to_string()),
                TemplateBlock::fresh("box".to_string(), "Box".to_string()),
            ],
        };
        t.blocks[1].style = BlockStyle::Box;
        t.blocks[1].parts = vec![BlockPart {
            key: "box".into(),
            text: "GM only.".into(),
            view: "gm".into(),
            edit: "gm".into(),
        }];
        t.normalize();
        t
    }

    #[test]
    fn a_type_may_reference_itself() {
        // A place lies within a place, a character knows a character.
        let t = probe();
        assert_eq!(t.field("g").unwrap().kind.references(), Some("probe"));
        t.validate().expect("a reference to itself is allowed");
    }

    #[test]
    fn a_fresh_type_without_parts_is_valid() {
        let empty = Template {
            id: "character".into(),
            name: "Character".into(),
            supertype: String::new(),
            group: false,
            navigation_hidden: false,
            rules: Vec::new(),
            icon: "📄".into(),
            description: String::new(),
            auth: None,
            name_pattern: just_input(),
            order: Vec::new(),
            sort: Vec::new(),
            create: String::new(),
            page: String::new(),
            properties: Vec::new(),
            infobox: Infobox::default(),
            blocks: Vec::new(),
        };
        empty.validate().expect("one has to be able to start from nothing");
    }

    #[test]
    fn duplicate_field_keys_are_caught() {
        let template = Template {
            id: "test".into(),
            name: "Test".into(),
            supertype: String::new(),
            group: false,
            navigation_hidden: false,
            rules: Vec::new(),
            icon: String::new(),
            description: String::new(),
            auth: None,
            name_pattern: just_input(),
            order: Vec::new(),
            sort: Vec::new(),
            create: String::new(),
            page: String::new(),
            properties: vec![
                    TemplateField::new("name", "Name", FieldKind::Text { multiline: false }),
                    TemplateField::new("name", "Name again", FieldKind::Text { multiline: false }),
                ],
            infobox: Infobox {
                ..Infobox::default()
            },
            blocks: Vec::new(),
        };
        assert_eq!(
            template.validate(),
            Err(TemplateError::DuplicateField("name".into()))
        );
    }

    #[test]
    fn a_reference_without_a_target_is_rejected() {
        let template = Template {
            id: "test".into(),
            name: "Test".into(),
            supertype: String::new(),
            group: false,
            navigation_hidden: false,
            rules: Vec::new(),
            icon: String::new(),
            description: String::new(),
            auth: None,
            name_pattern: just_input(),
            order: Vec::new(),
            sort: Vec::new(),
            create: String::new(),
            page: String::new(),
            properties: vec![TemplateField::new("x", "X", FieldKind::Reference { article_type: String::new(), multiple: false, parent: false, adds: false, count: String::new(), every: false, from: String::new() })],
            infobox: Infobox {
                ..Infobox::default()
            },
            blocks: Vec::new(),
        };
        assert_eq!(template.validate(), Err(TemplateError::NoTargetType("x".into())));
    }

    fn parent_field(key: &str, multiple: bool) -> TemplateField {
        TemplateField::new(
            key,
            key,
            FieldKind::Reference { article_type: String::new(), multiple, parent: true, adds: false, count: String::new(), every: false, from: String::new() },
        )
    }

    #[test]
    fn a_parent_is_single_and_unique() {
        let mut t = probe();
        t.properties = vec![parent_field("a", true)];
        assert_eq!(t.validate(), Err(TemplateError::MultipleParent("a".into())));

        t.properties = vec![parent_field("a", false), parent_field("b", false)];
        assert_eq!(
            t.validate(),
            Err(TemplateError::TwoParents("a".into(), "b".into()))
        );

        t.properties = vec![parent_field("a", false)];
        assert_eq!(t.validate(), Ok(()), "a bare parent needs no type");
    }

    #[test]
    fn ids_are_checked() {
        assert!(valid_slug("session-log"));
        assert!(!valid_slug("Session Log"));
        assert!(!valid_slug(""));
        assert!(!valid_slug("Place"));
    }

    #[test]
    fn normalising_is_repeatable() {
        // Normalising twice must not shift anything further.
        let mut t = probe();
        t.normalize();
        let after_once = t.clone();
        t.normalize();
        assert_eq!(t, after_once);
    }

    #[test]
    fn json_round_trip() {
        for template in [probe()] {
            let mut before = template.clone();
            before.normalize();
            let json = serde_json::to_string(&before).unwrap();
            let mut again: Template = serde_json::from_str(&json).unwrap();
            again.normalize();
            assert_eq!(again, before, "for '{}'", template.id);
        }
    }
}
