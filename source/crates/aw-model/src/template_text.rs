//! The template as text — in tags, like HTML.
//!
//! A type of article is defined rarely, and mostly by someone who knows what
//! they are building. For that one step typing is faster than clicking, and
//! the text can be copied, sent and compared.
//!
//! **What is stored is still the structure, not the text.** This file is only
//! a translation both ways; a click editor on the same data can stand beside
//! it at any time without the two drifting apart.
//!
//! ```text
//! ---
//! Portrait: <image size=200x260>
//! Species: <ref type=Species>
//! Age: <number unit=years>
//! Lies in: <ref parent>
//! Status: alive
//! ---
//!
//! <article view=all edit=gm>
//!   <name>
//!   <body>
//!     <infobox align=right width=narrow>
//!       <image bind=Portrait>
//!       <select bind=Species>
//!       <number bind=Age>
//!     </infobox>
//!
//!     <block title="History" view=players>
//!       %%Age%% years ago she came to @@Place:Morningstone@@.
//!       <image size=300x200 align=right>
//!       The text runs around the picture.
//!     </block>
//!   </body>
//! </article>
//! ```
//!
//! The properties on top are the data every document of the type has — a
//! kind and a default each, as a document writes its values. Everything
//! below is what is shown: an element with `bind=` shows a property and lets
//! it be changed while editing, `%%Name%%` reads it in the text.
//!
//! Tags instead of lines, because that makes the nesting visible: what is in
//! a block stands between its tags. That is also what lets an `<image>` sit
//! in the middle of flowing text — as a line of its own it could not, since
//! there it is content and not a field.
//!
//! The names are English and neutral: the application is tied neither to one
//! rule set nor to one language.
//!
//! On safety: a template holds only a **default** for new articles, never an
//! effective permission — that comes into being when an article is created
//! and is a field on the block afterwards. A mistyped attribute closes rather
//! than opens: nobody holds `gmm`, so nobody can read it.

use crate::template::{
    check_pattern, just_input, BlockFold, BlockPlace, BlockStyle, BlockPart, FieldKind, Infobox, Access, Rule, Template,
    TemplateBlock, TemplateField, PATTERN_INPUT,
};

/// Which types exist — id and name.
///
/// The text carries the **name** of a type (`type=Place`); what is stored is
/// its **id**. The directory translates between the two. Without it a
/// reference could not be written without showing the id — and that is a
/// string that means nothing to anyone.
#[derive(Clone, Debug, Default)]
pub struct Directory(Vec<(String, String)>);

impl Directory {
    pub fn new(entries: impl IntoIterator<Item = (String, String)>) -> Self {
        Directory(entries.into_iter().collect())
    }

    /// The name for an id — for writing.
    pub fn name(&self, id: &str) -> Option<&str> {
        self.0
            .iter()
            .find(|(k, _)| k == id)
            .map(|(_, n)| n.as_str())
    }

    /// The id for a name — for reading. Case does not matter: whoever types
    /// “place” means “Place”.
    pub fn id(&self, name: &str) -> Option<&str> {
        self.0
            .iter()
            .find(|(_, n)| n.eq_ignore_ascii_case(name.trim()))
            .map(|(k, _)| k.as_str())
    }

    pub fn names(&self) -> Vec<&str> {
        self.0.iter().map(|(_, n)| n.as_str()).collect()
    }
}

/// An error while reading, with the line it stands on.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TextError {
    /// 1-based, the way the editor counts.
    pub line: usize,
    pub text: String,
}

impl std::fmt::Display for TextError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Line {}: {}", self.line, self.text)
    }
}

const FIELD_TAGS: &str = "text, textarea, number, date, datetime, image";

/// The elements that show a property: `<select bind=Species>`.
const BIND_TAGS: &str = "text, textarea, number, date, select, image";

/// Whether an element can show a property of this kind.
pub fn shows(element: &str, kind: &FieldKind) -> bool {
    match element {
        "text" | "textarea" => matches!(kind, FieldKind::Text { .. } | FieldKind::RichText | FieldKind::Heading { .. }),
        "number" => matches!(kind, FieldKind::Number { .. }),
        "date" => matches!(kind, FieldKind::WorldDate | FieldKind::DateTime),
        "select" => matches!(kind, FieldKind::Reference { .. } | FieldKind::Relation { .. }),
        "image" => matches!(kind, FieldKind::Image { .. }),
        _ => false,
    }
}

/// The element that shows a property of this kind.
pub fn element_for(kind: &FieldKind) -> &'static str {
    match kind {
        FieldKind::Text { multiline: true } | FieldKind::RichText => "textarea",
        FieldKind::Text { .. } | FieldKind::Heading { .. } => "text",
        FieldKind::Number { .. } => "number",
        FieldKind::WorldDate | FieldKind::DateTime => "date",
        FieldKind::Reference { .. } | FieldKind::Relation { .. } => "select",
        FieldKind::Image { .. } => "image",
        FieldKind::Group { .. } => "group",
    }
}

// --- Writing ------------------------------------------------------------

/// Writes the infobox and blocks of a template as text.
pub fn to_text(template: &Template, directory: &Directory) -> String {
    if template.group {
        return group_to_text(template, directory);
    }
    let mut out = String::new();
    let applies = template.auth.clone().unwrap_or_default();

    // The properties on top, the way a document has them — and the rules.
    if !template.properties.is_empty() || !template.rules.is_empty() {
        out.push_str("---\n");
        for field in &template.properties {
            out.push_str(&property_line(field, directory));
            out.push('\n');
        }
        for rule in &template.rules {
            out.push_str(&rule_line(rule));
            out.push('\n');
        }
        out.push_str("---\n\n");
    }

    // The skeleton is always there — it is the ground of every article and
    // should not have to be guessed. That includes the access on <article>:
    // without it `all`/`all` applies, and that is a statement that should
    // stand there rather than be silently assumed.
    let article = access_on("article", &applies);
    if template.create.is_empty() {
        out.push_str(&format!("{article}\n"));
    } else {
        // Who may make documents of it, beside who sees and changes them.
        out.push_str(&format!("{} create={}>\n", article.trim_end_matches('>'), value(&template.create)));
    }

    let inside = "  ".to_string();
    // The name carries no access of its own: whoever may see the article sees
    // its name, and whoever may not does not find the article at all. Two
    // statements for one decision would be one too many.
    if template.name_pattern == PATTERN_INPUT {
        out.push_str(&format!("{inside}<name>\n"));
    } else {
        out.push_str(&format!("{inside}<name={}>\n", value(&template.name_pattern)));
    }
    out.push_str(&format!("{inside}<body>\n"));

    let margin: &str = &format!("{inside}  ");

    // An infobox showing nothing is not written: otherwise it would stand in
    // every fresh template and look as though it belonged.
    if !template.infobox.show.is_empty() {
        let access = template.infobox.access.as_ref().map(access_attributes).unwrap_or_default();
        out.push_str(&format!(
            "{margin}<infobox align={} width={}{access}>\n",
            template.infobox.align, template.infobox.width
        ));
        for shown in &template.infobox.show {
            let label = if shown.label.is_empty() {
                String::new()
            } else {
                format!(" label={}", value(&shown.label))
            };
            out.push_str(&format!(
                "{margin}  <{} bind={}{label}>\n",
                shown.element,
                value(&shown.property)
            ));
        }
        out.push_str(&format!("{margin}</infobox>\n"));
    }
    // By title: whoever writes the template names its blocks so.
    if !template.order.is_empty() {
        out.push_str(&format!("{margin}<order of={}>\n", value(&template.order.join(", "))));
    }
    if !template.sort.is_empty() {
        out.push_str(&format!("{margin}<sort by={}>\n", value(&template.sort.join(", "))));
    }

    for block in &template.blocks {
        if !out.ends_with(">\n") {
            out.push('\n');
        }

        // If every part has the same access, it stands on the block itself —
        // the way anyone would have written it. Only when the access changes
        // *inside* the block does it move in.
        let first = access_of(&block.parts[0]);
        let uniform = block.parts.iter().all(|t| access_of(t) == first);
        let own = (uniform && first != applies).then_some(&first);

        let (block_indent, depth) = (margin.to_string(), margin.len() + 2);
        out.push_str(&format!("{block_indent}{}\n", block_tag(block, own)));
        for part in &block.parts {
            let own = access_of(part);
            if uniform || own == applies {
                indent(&mut out, &part.text, depth);
            } else if let Some(element) = element_with_access(&part.text, &own) {
                out.push_str(&format!("{}{element}\n", " ".repeat(depth)));
            } else {
                out.push_str(&format!("{}{}\n", " ".repeat(depth), access_on("block", &own)));
                indent(&mut out, &part.text, depth + 2);
                out.push_str(&format!("{}</block>\n", " ".repeat(depth)));
            }
        }
        out.push_str(&format!("{block_indent}</block>\n"));
    }
    out.push_str(&format!("{inside}</body>\n"));
    out.push_str("</article>\n");
    out
}

fn indent(out: &mut String, text: &str, depth: usize) {
    let margin = " ".repeat(depth);
    for line in text.lines() {
        if line.trim().is_empty() {
            out.push('\n');
        } else {
            out.push_str(&format!("{margin}{line}\n"));
        }
    }
}

fn access_of(part: &BlockPart) -> Access {
    Access {
        view: part.view.clone(),
        edit: part.edit.clone(),
    }
}

/// Looks a type name up in the directory.
///
/// The text carries the name, what is stored is the id. If something stands
/// there that does not exist, that is an error with a line number — it used
/// to be a reference into nothing that nobody noticed.
fn id_for(name: &str, directory: &Directory) -> Result<String, String> {
    if let Some(id) = directory.id(name) {
        return Ok(id.to_string());
    }
    // The id itself is accepted too: in the round trip of a type the
    // directory does not know (yet), it would otherwise get lost.
    if directory.name(name).is_some() {
        return Ok(name.to_string());
    }
    let names = directory.names();
    Err(if names.is_empty() {
        format!("There is no type “{name}” — there are none at all yet.")
    } else {
        format!("There is no type “{name}”. These exist: {}.", names.join(", "))
    })
}

/// A tag that says nothing but access: `<article view=… edit=…>`, or a
/// region of the text, `<block view=… edit=…>`.
pub(crate) fn access_on(name: &str, access: &Access) -> String {
    format!("<{name}{}>", access_attributes(access))
}

/// ` view=… edit=…`.
fn access_attributes(access: &Access) -> String {
    format!(" view={} edit={}", value(&access.view), value(&access.edit))
}

/// Writes an attribute value — in quotes where needed.
pub(crate) fn value(text: &str) -> String {
    // Markdown takes a tag for a tag only when an unquoted value holds none
    // of `"'=<>` and backtick — `where=Owner=Ilva` would be text.
    if text.is_empty() || text.chars().any(|c| c.is_whitespace() || "\"'=<>`".contains(c)) {
        format!("{text:?}")
    } else {
        text.to_string()
    }
}

/// A property as a line on top: `Age: number years = 30`,
/// `Status: @@Life@@ = alive`, `Title:`.
fn property_line(field: &TemplateField, directory: &Directory) -> String {
    let show = |id: &String| directory.name(id).unwrap_or(id).to_string();
    let mut spec = match &field.kind {
        // Plain text needs no word — unless something follows it.
        FieldKind::Text { multiline: false } | FieldKind::Heading { .. } => {
            if field.access.is_none() && field.default.is_empty() { String::new() } else { "text".into() }
        }
        FieldKind::Text { multiline: true } | FieldKind::RichText => "textarea".into(),
        FieldKind::Number { unit } => match unit {
            Some(u) if !u.is_empty() => format!("number {}", value(u)),
            _ => "number".into(),
        },
        FieldKind::WorldDate => "date".into(),
        FieldKind::DateTime => "datetime".into(),
        FieldKind::Reference { article_type, parent: true, .. } if article_type.is_empty() => "parent".into(),
        FieldKind::Reference { article_type, multiple, parent, adds, count, every, from } => format!(
            "@@{}@@{}{}{}{}{}",
            show(article_type),
            if *every { " every" } else if *multiple { " multiple" } else { "" },
            if *parent { " parent" } else { "" },
            if *adds { " adds" } else { "" },
            if count.is_empty() { String::new() } else { format!(" with {}", value(count)) },
            if from.is_empty() { String::new() } else { format!(" from {}", value(from)) }
        ),
        FieldKind::Relation { article_type, role_type } => {
            format!("@@{}@@ as @@{}@@", show(article_type), show(role_type))
        }
        // `Abilities: group` — a group under its own name needs no other.
        FieldKind::Group { group } => {
            let name = show(group);
            if name.eq_ignore_ascii_case(&field.label) { "group".into() } else { format!("group {}", value(&name)) }
        }
        FieldKind::Image { width, height, align } => {
            let mut out = String::from("image");
            if *width > 0 || *height > 0 {
                out.push_str(&format!(" {width}x{height}"));
            }
            if align != "center" {
                out.push_str(&format!(" {align}"));
            }
            out
        }
    };
    if let Some(access) = &field.access {
        spec.push_str(&access_attributes(access));
    }
    if !field.default.is_empty() {
        spec.push_str(&format!(" = {}", field.default));
    }
    let spec = spec.trim();
    if spec.is_empty() {
        format!("{}:", field.label)
    } else {
        format!("{}: {spec}", field.label)
    }
}

pub(crate) fn block_tag(block: &TemplateBlock, access: Option<&Access>) -> String {
    let mut tag = String::from("<block");
    if !block.title.is_empty() {
        tag.push_str(&format!(" title={}", value(&block.title)));
    }
    match block.style {
        BlockStyle::Text => {}
        BlockStyle::Box => tag.push_str(" style=box"),
        BlockStyle::Quote => tag.push_str(" style=quote"),
        BlockStyle::Statblock => tag.push_str(" style=statblock"),
    }
    match block.place {
        BlockPlace::Flow => {}
        BlockPlace::Top => tag.push_str(" top"),
        BlockPlace::Bottom => tag.push_str(" bottom"),
        BlockPlace::Side => tag.push_str(" side"),
    }
    if block.width > 0 {
        tag.push_str(&format!(" width={}", block.width));
    }
    match block.fold {
        BlockFold::Open => {}
        BlockFold::Folded => tag.push_str(" fold=folded"),
        BlockFold::Off => tag.push_str(" fold=off"),
    }
    if block.heading != 1 {
        tag.push_str(&format!(" heading={}", block.heading));
    }
    if !block.new.is_empty() {
        tag.push_str(&format!(" new={}", value(&block.new.join(", "))));
    }
    // If the whole block has its own access, it stands on its tag. A bracket
    // around it would be one line more, one indent more and one ending more —
    // for a statement that belongs to the block. `<auth>` stays for what an
    // attribute cannot do: a region *inside* the flowing text.
    if let Some(r) = access {
        tag.push_str(&format!(" view={} edit={}", value(&r.view), value(&r.edit)));
    }
    tag.push('>');
    tag
}

// --- Reading ------------------------------------------------------------

/// A tag that was read.
#[derive(Clone, Debug)]
pub(crate) struct Tag {
    pub(crate) name: String,
    /// The value of the tag itself: `<name="Session log %%count%%">`.
    ///
    /// Only the skeleton uses this. A tag that carries exactly one value
    /// should not have to name it first — `<name pattern="…">` says the same
    /// thing twice.
    pub(crate) value: Option<String>,
    pub(crate) attributes: Vec<(String, String)>,
    pub(crate) closing: bool,
    line: usize,
}

impl Tag {
    pub(crate) fn attributes_mut(&mut self) -> impl Iterator<Item = (&String, &mut String)> {
        self.attributes.iter_mut().map(|(n, v)| (&*n, v))
    }

    pub(crate) fn attr(&self, name: &str) -> Option<&str> {
        self.attributes
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, w)| w.as_str())
    }
}

enum Piece {
    Tag(Tag),
    Text { text: String, line: usize },
}

/// Splits the text into tags and everything in between.
fn split(text: &str) -> Vec<Piece> {
    let chars: Vec<char> = text.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    let mut line = 1;
    let mut buffer = String::new();
    let mut buffer_line = 1;

    while i < chars.len() {
        if chars[i] == '<' {
            // A comment as in HTML — skipped over.
            // A note may span several lines — but only once it is closed. A
            // `<!--` just typed is not one yet and stays text; otherwise it
            // would swallow everything below and the skeleton would count as
            // deleted.
            if chars[i..].starts_with(&['<', '!', '-', '-']) {
                let rest: String = chars[i..].iter().collect();
                if let Some(p) = rest.find("-->") {
                    let end = (i + rest[..p + 3].chars().count()).min(chars.len());
                    line += chars[i..end].iter().filter(|c| **c == '\n').count();
                    i = end;
                    continue;
                }
            }
            // A tag ends on its own line. Whoever types a `<` and is not
            // finished should not catch the next `>` further down — otherwise
            // a half-typed `<` swallows the `</body>` below it, and the
            // skeleton counts as deleted.
            let to_line_end = chars[i..]
                .iter()
                .position(|c| *c == '\n')
                .unwrap_or(chars.len() - i);
            // And it holds no second `<`. If one stands inside, the first is
            // not a tag but a typed character — as in `<    </body>`, when
            // someone starts typing before the end tag.
            let tag = chars[i..i + to_line_end]
                .iter()
                .position(|c| *c == '>')
                .filter(|close| !chars[i + 1..i + close].contains(&'<'));
            if let Some(close) = tag {
                // Whitespace between two tags is passed on too: inside flowing
                // text it is content — the line break between `<list>` and
                // `</list>`, say. Outside it is ignored where it is read.
                if !buffer.is_empty() {
                    out.push(Piece::Text {
                        text: std::mem::take(&mut buffer),
                        line: buffer_line,
                    });
                }
                let raw: String = chars[i + 1..i + close].iter().collect();
                out.push(Piece::Tag(read_tag(&raw, line)));
                line += chars[i..=i + close].iter().filter(|c| **c == '\n').count();
                i += close + 1;
                buffer_line = line;
                continue;
            }
        }
        if chars[i] == '\n' {
            line += 1;
        }
        if buffer.is_empty() {
            buffer_line = line;
        }
        buffer.push(chars[i]);
        i += 1;
    }
    if !buffer.trim().is_empty() {
        out.push(Piece::Text {
            text: buffer,
            line: buffer_line,
        });
    }
    out
}

pub(crate) fn read_tag(raw: &str, line: usize) -> Tag {
    let raw = raw.trim().trim_end_matches('/').trim();
    let (closing, body) = match raw.strip_prefix('/') {
        Some(rest) => (true, rest.trim()),
        None => (false, raw),
    };

    // The name ends at a space — or at the `=` of a value of its own.
    let chars: Vec<char> = body.chars().collect();
    let mut i = 0;
    while i < chars.len() && !chars[i].is_whitespace() && chars[i] != '=' {
        i += 1;
    }
    let name: String = chars[..i].iter().collect::<String>().to_lowercase();

    let value = if i < chars.len() && chars[i] == '=' {
        i += 1;
        Some(read_value(&chars, &mut i))
    } else {
        None
    };
    let rest: String = chars[i..].iter().collect();

    Tag {
        name,
        value,
        attributes: read_attributes(&rest),
        closing,
        line,
    }
}

/// Reads a value — in quotes, or up to the next space.
fn read_value(chars: &[char], i: &mut usize) -> String {
    if *i < chars.len() && chars[*i] == '"' {
        *i += 1;
        let start = *i;
        while *i < chars.len() && chars[*i] != '"' {
            *i += 1;
        }
        let w: String = chars[start..*i].iter().collect();
        *i += 1;
        w
    } else {
        let start = *i;
        while *i < chars.len() && !chars[*i].is_whitespace() {
            *i += 1;
        }
        chars[start..*i].iter().collect()
    }
}

/// Reads `name=value`, `name="value with spaces"` and bare switches.
fn read_attributes(rest: &str) -> Vec<(String, String)> {
    let chars: Vec<char> = rest.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;

    while i < chars.len() {
        while i < chars.len() && chars[i].is_whitespace() {
            i += 1;
        }
        let start = i;
        while i < chars.len() && !chars[i].is_whitespace() && chars[i] != '=' {
            i += 1;
        }
        if i == start {
            break;
        }
        let name: String = chars[start..i].iter().collect::<String>().to_lowercase();

        if i < chars.len() && chars[i] == '=' {
            i += 1;
            let w = read_value(&chars, &mut i);
            out.push((name, w));
        } else {
            // A bare switch such as `multiple`.
            out.push((name, String::new()));
        }
    }
    out
}

/// How often each piece of the skeleton stands there.
///
/// Each should stand there exactly once. If one is missing or doubled, the
/// skeleton is not intact — the editor then puts it back.
#[derive(Default)]
struct Skeleton {
    article: usize,
    close_article: usize,
    name: usize,
    body: usize,
    close_body: usize,
}

impl Skeleton {
    fn intact(&self) -> bool {
        self.article == 1
            && self.close_article == 1
            && self.name == 1
            && self.body == 1
            && self.close_body == 1
    }
}

/// A property used somewhere: `<select bind=Species>` or `%%Species%%`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Use {
    /// The element that binds it — `None` for `%%…%%`, which only reads.
    pub element: Option<String>,
    pub property: String,
    pub line: usize,
}

pub struct Parsed {
    /// The properties on top — the data.
    pub properties: Vec<TemplateField>,
    /// The rules among them: `mod(x) = floor((x - 10) / 2)`.
    pub rules: Vec<Rule>,
    /// Every place a property is used. Whether it exists is checked where
    /// the inherited ones are known too: see `editor::read`.
    pub uses: Vec<Use>,
    /// How the name of a new article is built.
    pub name_pattern: String,
    /// The keys someone wrote down themselves.
    ///
    /// They are a decision and are not touched again — all others came from
    /// a title or a label.
    pub own_keys: Vec<String>,
    pub infobox: Infobox,
    pub blocks: Vec<TemplateBlock>,
    /// `<order of="Stats, Goals, …">`: the blocks by title, the inherited
    /// among them, and its line. Which block a title means is found where
    /// the inherited ones are known: see `editor::read`.
    pub order: Option<(usize, Vec<String>)>,
    /// `<sort by="Date, name">`: the properties its documents line up by,
    /// and its line — found by name in `editor::read`.
    pub sort: Option<(usize, Vec<String>)>,
    /// Bracket around the whole template.
    pub auth: Option<Access>,
    /// `create=` on `<article>`: who besides the GM may make documents of it.
    pub create: String,
    pub errors: Vec<TextError>,
    /// Whether the skeleton stood there complete.
    ///
    /// `false` means it was deleted in whole or in part. What stands here is
    /// parsed anyway — only the text has to be written afresh afterwards so
    /// the skeleton is there again.
    pub skeleton_intact: bool,
}

/// Reads the infobox and blocks out of text.
///
/// **Always** returns what was understood — and the errors beside it. While
/// typing, a half-finished line is the normal case; the preview should keep
/// up and not freeze on the last intact state. Whoever wants to be strict
/// checks the error list.
///
/// The whole input is always parsed: whoever has three typos should see them
/// in one pass and not one after another.
pub fn from_text(text: &str, directory: &Directory) -> Parsed {
    let mut errors: Vec<TextError> = Vec::new();
    // The lines on top are cut off first — and left as empty lines, so every
    // line below keeps its number.
    let (front, text) = split_front(text, &mut errors);
    let mut infobox = Infobox {
        align: "none".into(),
        ..Infobox::default()
    };
    let mut uses: Vec<Use> = Vec::new();
    let mut blocks: Vec<TemplateBlock> = Vec::new();
    let mut in_infobox = false;
    let mut body: Vec<String> = Vec::new();
    // What `<article>` says — the default for everything that says nothing.
    let mut article = Access::default();
    let mut create = String::new();
    // The blocks open right now, each with the access that holds inside it.
    // The first is a section; every one inside it is a region of its text,
    // a part of its own. The innermost wins.
    let mut open: Vec<Open> = Vec::new();
    // What holds in each section for a part that says nothing of its own —
    // an empty section gets its one part with exactly that.
    let mut block_defaults: Vec<Access> = Vec::new();
    // How often the pieces of the skeleton stand there. It should be
    // changeable but not deletable — for that it has to be visible that one
    // is missing or doubled.
    let mut skeleton = Skeleton::default();
    let mut name_pattern = just_input();
    let mut own_keys: Vec<String> = Vec::new();
    let mut order: Option<(usize, Vec<String>)> = None;
    let mut sort: Option<(usize, Vec<String>)> = None;
    // Inside `<map>` … `</map>` a pin or region may say who sees it without
    // becoming a part here: the document moves it into one of its own when
    // saved (`document::split_secret_pins`), so the map stays whole.
    let mut in_map = false;

    for piece in split(&text) {
        match piece {
            Piece::Text { text, line } => {
                if !open.is_empty() {
                    uses.extend(reads_in(&text, line));
                    body.push(text);
                } else if !text.trim().is_empty() {
                    errors.push(TextError {
                        line,
                        text: format!(
                            "“{}” stands on its own. Flowing text goes between <block> and </block>.",
                            text.trim().lines().next().unwrap_or_default()
                        ),
                    });
                }
            }
            Piece::Tag(tag) => {
                if tag.closing {
                    match tag.name.as_str() {
                        "infobox" => in_infobox = false,
                        "block" if !open.is_empty() => close_one(&mut blocks, &mut body, &mut open),
                        "article" => skeleton.close_article += 1,
                        "body" => skeleton.close_body += 1,
                        "name" => {}
                        "map" if !open.is_empty() => {
                            in_map = false;
                            body.push("</map>".into());
                        }
                        // In flowing text a closing tag the template language
                        // does not know is text — `</list>`, say, which the
                        // display reads itself.
                        other if !open.is_empty() => body.push(format!("</{other}>")),
                        other => errors.push(TextError {
                            line: tag.line,
                            text: format!("</{other}> closes nothing that is open."),
                        }),
                    }
                    continue;
                }

                match tag.name.as_str() {
                    // The skeleton carries no content of its own — it only
                    // says what belongs to what, and <article> whose it is.
                    "article" => {
                        skeleton.article += 1;
                        match read_access(&tag, &Access::default()) {
                            Ok(access) => article = access,
                            Err(text) => errors.push(TextError { line: tag.line, text }),
                        }
                        create = tag.attr("create").unwrap_or_default().trim().to_string();
                        if !create.is_empty() && create.parse::<crate::attribute::Policy>().is_err() {
                            errors.push(TextError { line: tag.line, text: format!("create=“{create}” is no view — as players, or class:rogue OR gm.") });
                        }
                    }
                    "body" => skeleton.body += 1,
                    "name" => {
                        skeleton.name += 1;
                        if let Some(m) = &tag.value {
                            if let Err(text) = check_pattern(m) {
                                errors.push(TextError {
                                    line: tag.line,
                                    text,
                                });
                            }
                            name_pattern = m.clone();
                        }
                    }
                    "infobox" => {
                        unclosed(&open, &mut errors);
                        close_all(&mut blocks, &mut body, &mut open);
                        in_infobox = true;
                        // Like a block: who sees and changes its values.
                        if ACCESS.iter().any(|n| tag.attr(n).is_some()) {
                            match read_access(&tag, &article) {
                                Ok(access) => infobox.access = Some(access),
                                Err(text) => errors.push(TextError { line: tag.line, text }),
                            }
                        }
                        match read_head(&tag) {
                            Ok((align, width)) => {
                                infobox.align = align;
                                infobox.width = width;
                            }
                            Err(text) => errors.push(TextError {
                                line: tag.line,
                                text,
                            }),
                        }
                    }
                    "order" if open.is_empty() => {
                        in_infobox = false;
                        let names: Vec<String> = tag
                            .attr("of")
                            .unwrap_or_default()
                            .split(',')
                            .map(|n| n.trim().to_string())
                            .filter(|n| !n.is_empty())
                            .collect();
                        if names.is_empty() {
                            errors.push(TextError { line: tag.line, text: "<order> names no block: <order of=\"Stats, Goals\">.".into() });
                        } else if order.is_some() {
                            errors.push(TextError { line: tag.line, text: "<order> appears twice.".into() });
                        } else {
                            order = Some((tag.line, names));
                        }
                    }
                    "sort" if open.is_empty() => {
                        in_infobox = false;
                        let names: Vec<String> = tag.attr("by").unwrap_or_default().split(',').map(|n| n.trim().to_string()).filter(|n| !n.is_empty()).collect();
                        if names.is_empty() {
                            errors.push(TextError { line: tag.line, text: "<sort> names no property: <sort by=\"Date, name\">.".into() });
                        } else if sort.is_some() {
                            errors.push(TextError { line: tag.line, text: "<sort> appears twice.".into() });
                        } else {
                            sort = Some((tag.line, names));
                        }
                    }
                    // A block inside a block is a region of its text: what
                    // stands in it becomes a layer of its own.
                    "block" if !open.is_empty() => {
                        let here = open.last().map(|o| o.access.clone()).unwrap_or_default();
                        push_part(&mut blocks, &mut body, here.clone(), false);
                        let parts = blocks.last().map_or(0, |b| b.parts.len());
                        if let Some(n) = ["title", "style", "key"].into_iter().find(|n| tag.attr(n).is_some()) {
                            errors.push(TextError {
                                line: tag.line,
                                text: format!(
                                    "A block inside a block is a region of the text — it takes view and edit, no {n}."
                                ),
                            });
                        }
                        match read_access(&tag, &here) {
                            Ok(access) => open.push(Open { access, parts, line: tag.line }),
                            Err(text) => {
                                errors.push(TextError { line: tag.line, text });
                                open.push(Open { access: here, parts, line: tag.line });
                            }
                        }
                    }
                    "block" => {
                        in_infobox = false;
                        match read_block(&tag, &article) {
                            Ok((block, access)) => {
                                if tag.attr("key").is_some_and(|k| !k.is_empty()) {
                                    own_keys.push(block.key.clone());
                                }
                                blocks.push(block);
                                block_defaults.push(access.clone());
                                open.push(Open { access, parts: 0, line: tag.line });
                            }
                            Err(text) => errors.push(TextError {
                                line: tag.line,
                                text,
                            }),
                        }
                    }
                    // In flowing text tags stay put — there only the display
                    // reads them, e.g. `<image>` in the middle of a paragraph.
                    // One that says who sees it is a part of its own, like a
                    // region holding only this element.
                    _ if !open.is_empty() => {
                        if let Some(property) = tag.attr("bind") {
                            uses.push(Use {
                                element: Some(tag.name.clone()),
                                property: property.to_string(),
                                line: tag.line,
                            });
                        }
                        if tag.name == "map" {
                            in_map = true;
                            // Its pins would be split from it: a map with
                            // access of its own stands in a block of its own.
                            if ACCESS.iter().any(|n| tag.attr(n).is_some()) {
                                errors.push(TextError {
                                    line: tag.line,
                                    text: "A map takes the access of its block — put it in <block view=…>, or give its pins their own.".into(),
                                });
                            }
                        }
                        if in_map || !ACCESS.iter().any(|n| tag.attr(n).is_some()) {
                            body.push(tag_back(&tag));
                            continue;
                        }
                        let here = open.last().map(|o| o.access.clone()).unwrap_or_default();
                        push_part(&mut blocks, &mut body, here.clone(), false);
                        match read_access(&tag, &here) {
                            Ok(access) => {
                                let mut bare = tag.clone();
                                bare.attributes.retain(|(n, _)| !ACCESS.contains(&n.as_str()));
                                body.push(tag_back(&bare));
                                push_part(&mut blocks, &mut body, access, true);
                            }
                            Err(text) => errors.push(TextError { line: tag.line, text }),
                        }
                    }
                    // The infobox holds no data — it shows properties.
                    _ if in_infobox && !BIND_TAGS.split(", ").any(|t| t == tag.name) => errors.push(TextError {
                        line: tag.line,
                        text: format!("There is no <{}> in an infobox. There is: {BIND_TAGS}.", tag.name),
                    }),
                    _ if in_infobox => match tag.attr("bind").filter(|b| !b.trim().is_empty()) {
                        Some(property) => {
                            if ACCESS.iter().any(|n| tag.attr(n).is_some()) {
                                errors.push(TextError { line: tag.line, text: ACCESS_ON_PROPERTY.into() });
                            }
                            uses.push(Use {
                                element: Some(tag.name.clone()),
                                property: property.trim().to_string(),
                                line: tag.line,
                            });
                            infobox.show.push(crate::template::Shown {
                                element: tag.name.clone(),
                                property: property.trim().to_string(),
                                label: tag.attr("label").unwrap_or_default().to_string(),
                            });
                        }
                        None => errors.push(TextError {
                            line: tag.line,
                            text: format!(
                                "<{}> shows nothing — in the infobox every element binds a property: <{} bind=Name>.",
                                tag.name, tag.name
                            ),
                        }),
                    },
                    other => errors.push(TextError {
                        line: tag.line,
                        text: format!(
                            "<{other}> stands on its own — the infobox goes between <body> and </body>, text and images between <block> and </block>."
                        ),
                    }),
                }
            }
        }
    }
    unclosed(&open, &mut errors);
    close_all(&mut blocks, &mut body, &mut open);

    // The properties are read last: what they leave out of their access
    // comes from <article>, and that stands below them.
    let mut properties: Vec<TemplateField> = Vec::new();
    let mut rules: Vec<Rule> = Vec::new();
    for (line, raw) in front {
        if let Some(rule) = read_rule(&raw) {
            match rule {
                Ok(rule) if rules.iter().any(|r| r.name.eq_ignore_ascii_case(&rule.name)) => {
                    errors.push(TextError { line, text: format!("The rule {} appears twice.", rule.name) })
                }
                Ok(rule) => rules.push(rule),
                Err(text) => errors.push(TextError { line, text }),
            }
            continue;
        }
        match read_property(&raw, &article, directory) {
            Ok(field) => {
                if properties.iter().any(|p| p.label.eq_ignore_ascii_case(&field.label)) {
                    errors.push(TextError { line, text: format!("“{}” appears twice.", field.label) });
                    continue;
                }
                properties.push(field);
            }
            Err(text) => errors.push(TextError { line, text }),
        }
    }

    // Every block and every property gets an id — a fresh one here. Whoever
    // compares them with an earlier state replaces them straight away with
    // the known ones (see `editor::read`). Without this step a template read
    // on its own would have no ids at all.
    for field in &mut properties {
        if field.key.is_empty() {
            field.key = new_id();
        }
    }
    for block in &mut blocks {
        if block.key.is_empty() {
            block.key = new_id();
        }
        // The parts hang on the block's id — and that was not settled yet.
        for (n, part) in block.parts.iter_mut().enumerate() {
            part.key = TemplateBlock::part_key(&block.key, n);
        }
    }

    errors.extend(duplicates(
        properties.iter().map(|f| f.key.as_str()),
        "property key",
    ));
    errors.extend(duplicates(
        blocks.iter().map(|b| b.key.as_str()),
        "block key",
    ));

    if infobox.show.is_empty() {
        infobox.align = "none".into();
    }
    // What says the article's access says nothing of its own.
    if infobox.access.as_ref() == Some(&article) {
        infobox.access = None;
    }
    // A block without content still has one part — otherwise an article of
    // this type would have no room to write there at all.
    for (block, access) in blocks.iter_mut().zip(&block_defaults) {
        if block.parts.is_empty() {
            block.parts.push(BlockPart {
                view: access.view.clone(),
                edit: access.edit.clone(),
                ..BlockPart::fresh(block.key.clone())
            });
        }
    }

    // `view=all edit=all` restricts nothing. Inside it becomes `None` —
    // otherwise it would mean the same thing twice to every place that asks.
    let auth = Some(article).filter(|r| r != &Access::default());
    errors.sort_by_key(|e| e.line);
    Parsed {
        properties,
        rules,
        uses,
        infobox,
        blocks,
        order,
        sort,
        name_pattern,
        own_keys,
        auth,
        create,
        errors,
        skeleton_intact: skeleton.intact(),
    }
}

// --- Property groups -----------------------------------------------------

/// The key of a group's layout — its one block.
pub const LAYOUT: &str = "layout";

/// A group's layout: what `<view of=Group>` draws.
pub fn layout_of(group: &Template) -> &str {
    group.blocks.first().and_then(|b| b.parts.first()).map(|p| p.text.as_str()).unwrap_or_default()
}

/// A property group as text: its properties, one per line, and — if it has
/// one — its layout below them, the properties then between `---` as on
/// top of a type.
pub fn group_to_text(group: &Template, directory: &Directory) -> String {
    let lines: String = group
        .properties
        .iter()
        .map(|f| property_line(f, directory) + "\n")
        .chain(group.rules.iter().map(|r| rule_line(r) + "\n"))
        .collect();
    let layout = layout_of(group);
    if layout.trim().is_empty() {
        lines
    } else {
        format!("---\n{lines}---\n\n{}\n", layout.trim_end())
    }
}

/// A property group, read: [`group_from_text`].
pub struct GroupText {
    pub properties: Vec<TemplateField>,
    pub rules: Vec<Rule>,
    /// What `<view of=Group>` draws.
    pub layout: String,
    pub errors: Vec<TextError>,
}

/// Reads a property group: a property or a rule per line, as on top of a
/// type. Between `---`, the text below them is the group's layout: what
/// `<view of=Group>` draws, with the values of the document it stands in.
pub fn group_from_text(text: &str, directory: &Directory) -> GroupText {
    let mut properties: Vec<TemplateField> = Vec::new();
    let mut rules: Vec<Rule> = Vec::new();
    let mut errors = Vec::new();
    let lines: Vec<&str> = text.split('\n').collect();
    let fenced = lines.iter().position(|l| !l.trim().is_empty()).filter(|i| lines[*i].trim() == "---");
    let end = fenced.and_then(|start| lines[start + 1..].iter().position(|l| l.trim() == "---").map(|p| p + start + 1));
    let (front, layout) = match end {
        Some(end) => (&lines[..end], lines[end + 1..].join("\n").trim().to_string()),
        None => (&lines[..], String::new()),
    };
    for (i, raw) in front.iter().enumerate() {
        let raw = raw.trim();
        if raw.is_empty() || raw == "---" {
            continue;
        }
        let line = i + 1;
        if raw.starts_with('<') {
            errors.push(TextError { line, text: "Above the layout stand properties only — Name: kind = default. The layout goes below the closing ---.".into() });
            continue;
        }
        if let Some(rule) = read_rule(raw) {
            match rule {
                Ok(rule) if rules.iter().any(|r| r.name.eq_ignore_ascii_case(&rule.name)) => {
                    errors.push(TextError { line, text: format!("The rule {} appears twice.", rule.name) })
                }
                Ok(rule) => rules.push(rule),
                Err(text) => errors.push(TextError { line, text }),
            }
            continue;
        }
        match read_property(raw, &Access::default(), directory) {
            Ok(mut field) => {
                if properties.iter().any(|p| p.label.eq_ignore_ascii_case(&field.label)) {
                    errors.push(TextError { line, text: format!("“{}” appears twice.", field.label) });
                    continue;
                }
                field.key = new_id();
                properties.push(field);
            }
            Err(text) => errors.push(TextError { line, text }),
        }
    }
    GroupText { properties, rules, layout, errors }
}

/// A rule as it stands on top: `mod(x) = floor((x - 10) / 2)`, `crit = 20`.
fn rule_line(rule: &Rule) -> String {
    if rule.params.is_empty() {
        format!("{} = {}", rule.name, rule.formula)
    } else {
        format!("{}({}) = {}", rule.name, rule.params.join(", "), rule.formula)
    }
}

/// A rule line — `None` when the line is none: a rule has its `=` before
/// any colon, a property its colon first.
fn read_rule(raw: &str) -> Option<Result<Rule, String>> {
    let eq = raw.find('=')?;
    if raw.find(':').is_some_and(|colon| colon < eq) {
        return None;
    }
    let (head, formula) = (raw[..eq].trim(), raw[eq + 1..].trim());
    let (name, params) = match head.split_once('(') {
        Some((name, rest)) => {
            let Some(inner) = rest.trim_end().strip_suffix(')') else {
                return Some(Err(format!("“{head}” — the bracket is not closed: {}(x) = …", name.trim())));
            };
            let params = inner.split(',').map(|p| p.trim().to_string()).filter(|p| !p.is_empty()).collect();
            (name.trim(), params)
        }
        None => (head, Vec::new()),
    };
    let rule = Rule { name: name.to_string(), params, formula: formula.to_string() };
    Some(rule.validate().map(|_| rule).map_err(|e| e.to_string()))
}

/// The attributes that say who may see and change.
const ACCESS: [&str; 2] = ["view", "edit"];

/// What reading says to access on something that only shows a property.
const ACCESS_ON_PROPERTY: &str =
    "Who may see a property is said on the property itself, on top: Name: <text view=gm edit=gm>.";

/// Takes the lines between `---` off the top.
///
/// Returns them with their line numbers — and the text with those lines left
/// empty, so everything below keeps its number.
fn split_front(text: &str, errors: &mut Vec<TextError>) -> (Vec<(usize, String)>, String) {
    let lines: Vec<&str> = text.split('\n').collect();
    let Some(start) = lines.iter().position(|l| !l.trim().is_empty()).filter(|i| lines[*i].trim() == "---")
    else {
        return (Vec::new(), text.to_string());
    };
    let Some(end) = lines[start + 1..].iter().position(|l| l.trim() == "---").map(|p| p + start + 1) else {
        errors.push(TextError {
            line: start + 1,
            text: "The properties are not closed — a line with --- is missing below them.".into(),
        });
        return (Vec::new(), text.to_string());
    };
    let front = (start + 1..end)
        .filter(|i| !lines[*i].trim().is_empty())
        .map(|i| (i + 1, lines[i].trim().to_string()))
        .collect();
    let rest = lines
        .iter()
        .enumerate()
        .map(|(i, l)| if (start..=end).contains(&i) { "" } else { l })
        .collect::<Vec<_>>()
        .join("\n");
    (front, rest)
}

/// One property line: `Age: number years = 30`, `Status: @@Life@@ = alive`,
/// `Title:`.
///
/// After the colon: the kind — a word, or `@@Type@@` for a document of that
/// type — then its options and who may see it, then `=` and the default.
/// Nothing at all is text without a default.
fn read_property(raw: &str, article: &Access, directory: &Directory) -> Result<TemplateField, String> {
    let (name, rest) = raw
        .split_once(':')
        .ok_or_else(|| format!("“{raw}” is no property — write Name: kind = default."))?;
    let name = name.trim();
    if name.is_empty() {
        return Err(format!("“{raw}” has no name before the colon."));
    }
    let (spec, default) = split_default(rest);
    let spec = spec.trim();

    // The kind: `@@Type@@`, or the first word.
    let (kind_word, options) = if let Some(after) = spec.strip_prefix("@@") {
        let close = after
            .find("@@")
            .ok_or_else(|| format!("“{spec}” — the @@ after the type is missing."))?;
        (format!("@@{}", &after[..close]), &after[close + 2..])
    } else {
        let end = spec.find(char::is_whitespace).unwrap_or(spec.len());
        (spec[..end].to_string(), &spec[end..])
    };

    // `@@Character@@ as @@Relation@@` — the roles of a relation come from
    // a type of their own. Its name may hold spaces, so it is taken off
    // before the words are split.
    let after_as = options
        .trim_start()
        .strip_prefix("as")
        .filter(|rest| rest.is_empty() || rest.starts_with(char::is_whitespace));
    let (roles, options) = match after_as {
        Some(after) if kind_word.starts_with("@@") && after.trim_start().starts_with("@@") => {
            let after = after.trim_start().trim_start_matches("@@");
            let close = after
                .find("@@")
                .ok_or_else(|| format!("“{spec}” — the @@ after the roles is missing."))?;
            (Some(after[..close].trim().to_string()), &after[close + 2..])
        }
        Some(_) if kind_word.starts_with("@@") => {
            return Err(format!("after as comes the type of the roles: {name}: {kind_word}@@ as @@Relation@@."))
        }
        _ => (None, options),
    };

    // Who may see it stands among the options: view=, edit=.
    let mut access_part = String::new();
    let mut words: Vec<String> = Vec::new();
    for word in split_words(options) {
        if ACCESS.iter().any(|a| word.starts_with(&format!("{a}="))) {
            access_part.push(' ');
            access_part.push_str(&word);
        } else {
            words.push(word);
        }
    }
    let unexpected = |words: &[String], kind: &str| -> Result<(), String> {
        match words.first() {
            Some(w) => Err(format!("“{w}” means nothing after {kind}. A default goes after =: {name}: {kind} = …")),
            None => Ok(()),
        }
    };

    let kind = match kind_word.as_str() {
        "" | "text" => {
            unexpected(&words, "text")?;
            FieldKind::Text { multiline: false }
        }
        "textarea" => {
            unexpected(&words, "textarea")?;
            FieldKind::Text { multiline: true }
        }
        "date" => {
            unexpected(&words, "date")?;
            FieldKind::WorldDate
        }
        "datetime" => {
            unexpected(&words, "datetime")?;
            FieldKind::DateTime
        }
        // Everything after `number` is its unit.
        "number" => FieldKind::Number { unit: (!words.is_empty()).then(|| words.join(" ")) },
        // `Stats: group Abilities` — the properties of that group, here.
        // Without a name the group is called like the property.
        "group" => {
            if !default.trim().is_empty() {
                return Err(format!("{name} is a group — its properties have their defaults in the group."));
            }
            let of = if words.is_empty() { name.to_string() } else { words.join(" ") };
            FieldKind::Group { group: id_for(&of, directory)? }
        }
        // The tree within the own type.
        "parent" => {
            unexpected(&words, "parent")?;
            FieldKind::Reference { article_type: String::new(), multiple: false, parent: true, adds: false, count: String::new(), every: false, from: String::new() }
        }
        "image" => {
            let (mut width, mut height, mut align) = (0, 0, "center".to_string());
            for w in &words {
                if let Some((x, y)) = dimensions(w).filter(|_| !w.is_empty()) {
                    (width, height) = (x, y);
                } else if ["left", "center", "right", "none"].contains(&w.as_str()) {
                    align = w.clone();
                } else {
                    return Err(format!("“{w}” means nothing after image. There is a size like 200x260, and left, center, right, none."));
                }
            }
            FieldKind::Image { width, height, align }
        }
        // `@@Type@@` — one of the documents of that type, the way the value
        // in a document is a link to one of them.
        k if k.starts_with("@@") && roles.is_some() => {
            let article_type = id_for(k.trim_start_matches("@@").trim(), directory)?;
            let role_type = id_for(roles.as_deref().unwrap_or_default(), directory)?;
            if let Some(w) = words.first() {
                return Err(format!("“{w}” means nothing after a relation — it always holds several."));
            }
            if !default.trim().is_empty() {
                return Err(format!("{name} holds relations — they have no default."));
            }
            FieldKind::Relation { article_type, role_type }
        }
        k if k.starts_with("@@") => {
            let article_type = id_for(k.trim_start_matches("@@").trim(), directory)?;
            let (mut multiple, mut parent, mut adds, mut count, mut every, mut from) = (false, false, false, String::new(), false, String::new());
            let mut rest = words.iter();
            while let Some(w) = rest.next() {
                match w.as_str() {
                    "multiple" => multiple = true,
                    // Every document of the type, always — only their numbers change.
                    "every" => (every, multiple) = (true, true),
                    "parent" => parent = true,
                    "adds" => adds = true,
                    // `with Level`: a number of its own for every pick.
                    "with" => match rest.next() {
                        Some(name) => count = name.clone(),
                        None => return Err(format!("with names the number each pick has: {name}: {kind_word}@@ multiple with Level.")),
                    },
                    // `from Previous`: the documents whose Previous points here.
                    "from" => match rest.next() {
                        Some(name) => from = name.clone(),
                        None => return Err(format!("from names their property that points here: {name}: {kind_word}@@ from Previous.")),
                    },
                    other => return Err(format!("“{other}” means nothing after a type. There is: multiple, every, parent, adds, with Name, from Name, as @@Type@@.")),
                }
            }
            if multiple && parent {
                return Err("the parent points at exactly one document — leave out multiple.".into());
            }
            if parent && !count.is_empty() {
                return Err("the parent is where a document lies — it has no number of its own.".into());
            }
            if every && parent {
                return Err("the parent is one document — every means all of them.".into());
            }
            if every && count.is_empty() {
                return Err(format!("every needs a number for each: {name}: {kind_word}@@ every with Level."));
            }
            if !from.is_empty() && (parent || every || adds || !count.is_empty()) {
                return Err(format!("{name} only shows what points here — it holds nothing of its own."));
            }
            if !from.is_empty() && !default.trim().is_empty() {
                return Err(format!("{name} only shows what points here — it has no default."));
            }
            FieldKind::Reference { article_type, multiple, parent, adds, count, every, from }
        }
        other => {
            return Err(format!(
                "“{other}” is no kind. There is: {FIELD_TAGS}, group, @@Type@@ and parent — a default goes after =: {name}: text = {other}."
            ))
        }
    };

    let mut field = TemplateField::new(String::new(), name, kind);
    if !access_part.is_empty() && matches!(field.kind, FieldKind::Group { .. }) {
        return Err(format!("Who may see the properties of {name} is said in the group, on each of them."));
    }
    if !access_part.is_empty() {
        field.access = Some(read_access(&read_tag(&format!("x{access_part}"), 0), article)?);
    }
    field.default = default.trim().to_string();
    Ok(field)
}

/// Splits `= default` off: the first `=` that is no attribute's — no word
/// stands right before it.
fn split_default(rest: &str) -> (&str, &str) {
    let mut before = ' ';
    for (i, c) in rest.char_indices() {
        if c == '=' && !(before.is_alphanumeric() || before == '_' || before == '-') {
            return (&rest[..i], &rest[i + 1..]);
        }
        before = c;
    }
    (rest, "")
}

/// Words, with `"…"` holding spaces together.
fn split_words(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut word = String::new();
    let mut quoted = false;
    for c in text.chars() {
        match c {
            '"' => quoted = !quoted,
            c if c.is_whitespace() && !quoted => {
                if !word.is_empty() {
                    out.push(std::mem::take(&mut word));
                }
            }
            c => word.push(c),
        }
    }
    if !word.is_empty() {
        out.push(word);
    }
    out
}

/// The `%%Name%%` in a piece of flowing text, with the line each stands on.
///
/// `first` is the line the piece was given — a line break it starts with
/// already counts there.
fn reads_in(text: &str, first: usize) -> Vec<Use> {
    let first = first - usize::from(text.starts_with('\n'));
    let mut out = Vec::new();
    let mut from = 0;
    while let Some(open) = text[from..].find("%%").map(|i| i + from) {
        let Some(close) = text[open + 2..].find("%%").map(|i| i + open + 2) else { break };
        let name = &text[open + 2..close];
        if name.trim().is_empty() || name.contains('\n') {
            from = close;
            continue;
        }
        out.push(Use {
            element: None,
            property: name.trim().to_string(),
            line: first + text[..open].matches('\n').count(),
        });
        from = close + 2;
    }
    out
}

/// A part with access of its own that is a single element — `<image …>`,
/// `<list …>` — written with the access on the element itself. Anything else
/// needs an inner `<block view=… edit=…>` around it: `None`.
pub(crate) fn element_with_access(text: &str, access: &Access) -> Option<String> {
    let line = text.trim();
    let raw = line.strip_prefix('<')?.strip_suffix('>')?;
    if line.contains('\n') || raw.starts_with('/') || raw.starts_with('!') || raw.contains(['<', '>']) {
        return None;
    }
    let tag = read_tag(raw, 0);
    if matches!(tag.name.as_str(), "block" | "article" | "name" | "body" | "infobox" | "auth")
        || tag.name.is_empty()
        || ACCESS.iter().any(|n| tag.attr(n).is_some())
    {
        return None;
    }
    let written = tag_back(&tag);
    Some(format!("{}{}>", written.trim_end_matches('>'), access_attributes(access)))
}

/// A block that is open while reading.
struct Open {
    /// What holds inside it.
    access: Access,
    /// How many parts its section had when it opened.
    parts: usize,
    /// Where its tag stands — for the error when it is never closed.
    line: usize,
}

/// Closes the innermost open block.
///
/// A region is a part even when empty — in a template it is the room an
/// article writes in. Only when something inside it already became a part
/// (a region within the region) does nothing empty follow after it.
fn close_one(blocks: &mut [TemplateBlock], body: &mut Vec<String>, open: &mut Vec<Open>) {
    let Some(closing) = open.pop() else { return };
    let region = !open.is_empty();
    let untouched = blocks.last().map_or(0, |b| b.parts.len()) == closing.parts;
    push_part(blocks, body, closing.access, region && untouched);
}

/// Every block still open where it has to be closed is an error. What was
/// understood is kept all the same, so the preview keeps up.
fn unclosed(open: &[Open], errors: &mut Vec<TextError>) {
    for o in open {
        errors.push(TextError {
            line: o.line,
            text: "This <block> is not closed — a </block> is missing.".into(),
        });
    }
}

/// Closes whatever blocks are still open — at the end, or where an infobox
/// begins.
fn close_all(blocks: &mut [TemplateBlock], body: &mut Vec<String>, open: &mut Vec<Open>) {
    while !open.is_empty() {
        close_one(blocks, body, open);
    }
}

/// Writes a tag back the way it stood.
/// An `<image …>` standing on a line of its own: its attributes, in order.
pub(crate) fn image_line(line: &str) -> Option<Vec<(String, String)>> {
    let raw = line.trim().strip_prefix('<')?.strip_suffix('>')?;
    if raw.contains(['<', '>']) || raw.starts_with('/') {
        return None;
    }
    let tag = read_tag(raw, 0);
    (tag.name == "image").then_some(tag.attributes)
}

/// An `<image …>` with these attributes, as text.
pub(crate) fn image_with(attributes: &[(String, String)]) -> String {
    tag_back(&Tag { name: "image".into(), value: None, attributes: attributes.to_vec(), closing: false, line: 0 })
}

pub(crate) fn tag_back(tag: &Tag) -> String {
    let mut out = format!("<{}", tag.name);
    if let Some(w) = &tag.value {
        out.push_str(&format!("={}", value(w)));
    }
    for (name, w) in &tag.attributes {
        if w.is_empty() {
            out.push_str(&format!(" {name}"));
        } else {
            out.push_str(&format!(" {name}={}", value(w)));
        }
    }
    out.push('>');
    out
}

/// Stores the collected text as a part of the last block.
///
/// Text that stands nowhere is no part — unless it is a region, which is
/// one even when empty.
fn push_part(blocks: &mut [TemplateBlock], body: &mut Vec<String>, access: Access, region: bool) {
    let text = dedented(body);
    body.clear();
    if text.is_empty() && !region {
        return;
    }

    if let Some(block) = blocks.last_mut() {
        let key = TemplateBlock::part_key(&block.key, block.parts.len());
        block.parts.push(BlockPart {
            key,
            text,
            view: access.view,
            edit: access.edit,
        });
    }
}

/// Removes the common indent and trims the edges.
fn dedented(body: &[String]) -> String {
    if body.is_empty() {
        return String::new();
    }
    {
        // The common indent falls away — it belongs to the tag, not to the
        // text.
        let raw = body.join("");
        let lines: Vec<&str> = raw.lines().collect();
        let depth = lines
            .iter()
            .filter(|z| !z.trim().is_empty())
            .map(|z| z.len() - z.trim_start().len())
            .min()
            .unwrap_or(0);
        let text: Vec<String> = lines
            .iter()
            .map(|z| {
                if z.len() >= depth {
                    z[depth..].to_string()
                } else {
                    z.trim().to_string()
                }
            })
            .collect();
        text.join("\n").trim().to_string()
    }
}

/// Reads `<auth view=… edit=…>`.
///
/// What is not written comes from the surrounding bracket — and if there is
/// none, from the default: everyone may do everything.
pub(crate) fn read_access(tag: &Tag, fallback: &Access) -> Result<Access, String> {
    let mut access = fallback.clone();
    if let Some(w) = tag.attr("view").filter(|w| !w.is_empty()) {
        access.view = w.to_string();
    }
    if let Some(w) = tag.attr("edit").filter(|w| !w.is_empty()) {
        access.edit = w.to_string();
    }
    Ok(access)
}

fn duplicates<'a>(keys: impl Iterator<Item = &'a str>, what: &str) -> Vec<TextError> {
    let mut seen = std::collections::BTreeSet::new();
    let mut out = Vec::new();
    for key in keys {
        if !seen.insert(key.to_string()) {
            out.push(TextError {
                line: 0,
                text: format!("{what} “{key}” appears more than once."),
            });
        }
    }
    out
}

const ALIGNMENTS_TEXT: [&str; 4] = ["right", "left", "top", "none"];

fn read_head(tag: &Tag) -> Result<(String, String), String> {
    let align = tag.attr("align").unwrap_or("right");
    if !ALIGNMENTS_TEXT.contains(&align) {
        return Err(format!(
            "nobody knows align={align}. There is: right, left, top, none."
        ));
    }
    let width = tag.attr("width").unwrap_or("auto");
    if !crate::template::valid_width(width) {
        return Err(format!(
            "nobody knows width={width}. There is: auto, a number in pixels, narrow or wide."
        ));
    }
    Ok((align.into(), width.into()))
}

/// Gives the blocks that were read their ids back.
///
/// A block's id does **not** stand in the text — the layers of the
/// documents lie under it, and it has nothing to do with the text. So reading
/// has to decide which block that was read is which known one.
///
/// In two passes: first what matches, then what is left, in order. That way
/// a block survives renaming *and* moving — and whoever inserts a new one
/// gets a new id instead of the neighbour's.
fn assign_ids<T>(
    fresh: &mut [T],
    previous: &[T],
    same: impl Fn(&T, &T) -> bool,
    id: impl Fn(&T) -> String,
    set: impl Fn(&mut T, String),
) {
    let mut taken = vec![false; previous.len()];
    let mut mapping: Vec<Option<usize>> = vec![None; fresh.len()];

    // First pass: what matches belongs together.
    for (i, n) in fresh.iter().enumerate() {
        if let Some(j) = previous
            .iter()
            .enumerate()
            .position(|(j, a)| !taken[j] && same(n, a))
        {
            mapping[i] = Some(j);
            taken[j] = true;
        }
    }

    // Second pass: the rest in order. Whoever stands where they stood before
    // is the same one — only renamed.
    let mut free = (0..previous.len()).filter(|j| !taken[*j]);
    for i in 0..fresh.len() {
        if mapping[i].is_none() {
            mapping[i] = free.next();
        }
    }

    for (i, target) in mapping.into_iter().enumerate() {
        match target {
            Some(j) => set(&mut fresh[i], id(&previous[j])),
            // New: an id that never existed. Not from the title — otherwise a
            // second block “History” would inherit the layers of the first
            // one that someone deleted a year ago.
            None => set(&mut fresh[i], new_id()),
        }
    }
}

/// A fresh id. Never shown, never typed, never reused.
pub fn new_id() -> String {
    ulid::Ulid::generate().to_string().to_lowercase()
}

/// For the blocks — their parts hang on the block id and move with it.
pub fn assign_block_ids(fresh: &mut [TemplateBlock], previous: &[TemplateBlock]) {
    assign_ids(
        fresh,
        previous,
        |a, b| a.title == b.title && a.style == b.style,
        |b| b.key.clone(),
        |b, id| {
            for (n, part) in b.parts.iter_mut().enumerate() {
                part.key = TemplateBlock::part_key(&id, n);
            }
            b.key = id;
        },
    );
    // A block's parts keep the keys they had, in order — a reader may not
    // see every part, so a place in what they see is no key. What comes
    // after takes a key none had.
    for block in fresh.iter_mut() {
        let Some(before) = previous.iter().find(|p| p.key == block.key) else { continue };
        let had: Vec<&str> = before.parts.iter().map(|p| p.key.as_str()).collect();
        let mut next = had.len();
        for (n, part) in block.parts.iter_mut().enumerate() {
            part.key = match had.get(n) {
                Some(key) => key.to_string(),
                None => loop {
                    let key = TemplateBlock::part_key(&block.key, next);
                    next += 1;
                    if !had.contains(&key.as_str()) {
                        break key;
                    }
                },
            };
        }
    }
}

/// And for the fields of the infobox.
pub fn assign_field_ids(fresh: &mut [TemplateField], previous: &[TemplateField]) {
    assign_ids(
        fresh,
        previous,
        |a, b| a.label == b.label && a.kind == b.kind,
        |f| f.key.clone(),
        |f, id| f.key = id,
    );
}

/// Turns a label into a key.
///
/// So nobody has to invent one: `label="True goal"` becomes `true-goal`.
/// Umlauts are transliterated, since labels may be written in any language.
pub fn key_from(text: &str) -> String {
    let replaced = text
        .to_lowercase()
        .replace('ä', "ae")
        .replace('ö', "oe")
        .replace('ü', "ue")
        .replace('ß', "ss");
    let raw: String = replaced
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' {
                c
            } else {
                '-'
            }
        })
        .collect();
    // Collapse repeated hyphens, trim the edges.
    let mut out = String::new();
    for c in raw.chars() {
        if c == '-' && out.ends_with('-') {
            continue;
        }
        out.push(c);
    }
    out.trim_matches('-').to_string()
}

fn dimensions(rest: &str) -> Option<(u16, u16)> {
    if rest.is_empty() {
        return Some((0, 0));
    }
    let (w, h) = rest.split_once(['x', 'X', '×'])?;
    Some((w.trim().parse().ok()?, h.trim().parse().ok()?))
}

/// Reads a block tag — and the access that holds inside it: its own where
/// it says one, otherwise the article's.
fn read_block(tag: &Tag, article: &Access) -> Result<(TemplateBlock, Access), String> {
    let title = tag.attr("title").unwrap_or_default().to_string();
    // Here too: the id does not come from the title but from what stood there
    // before. The articles' layers lie under it; a title is a label and may
    // change.
    let mut block = TemplateBlock::fresh(String::new(), title);
    block.parts.clear();
    block.style = match tag.attr("style").unwrap_or("text") {
        "box" => BlockStyle::Box,
        "quote" => BlockStyle::Quote,
        "statblock" => BlockStyle::Statblock,
        "text" => BlockStyle::Text,
        other => {
            return Err(format!(
                "nobody knows style={other}. There is: text, box, quote, statblock."
            ))
        }
    };
    block.place = match (tag.attr("top").is_some(), tag.attr("bottom").is_some(), tag.attr("side").is_some()) {
        (false, false, false) => BlockPlace::Flow,
        (true, false, false) => BlockPlace::Top,
        (false, true, false) => BlockPlace::Bottom,
        (false, false, true) => BlockPlace::Side,
        _ => return Err("a block stands top, bottom or at the side — one of them.".into()),
    };
    block.width = match tag.attr("width").map(|w| w.trim().trim_end_matches("px")) {
        None | Some("") => 0,
        Some(w) => w.parse().map_err(|_| format!("width={w} is no number of pixels."))?,
    };
    block.fold = match tag.attr("fold").unwrap_or("open") {
        "open" | "" => BlockFold::Open,
        "folded" => BlockFold::Folded,
        "off" => BlockFold::Off,
        other => return Err(format!("nobody knows fold={other}. There is: open, folded, off.")),
    };
    block.new = tag.attr("new").unwrap_or_default().split(',').map(|t| t.trim().to_string()).filter(|t| !t.is_empty()).collect();
    if let Some(level) = tag.attr("heading") {
        block.heading = level
            .parse()
            .ok()
            .filter(|n| crate::template::HEADINGS.contains(n))
            .ok_or_else(|| format!("nobody knows heading={level}. There is: 1, 2, 3, 4."))?;
    }
    Ok((block, read_access(tag, article)?))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The types the fixtures refer to.
    ///
    /// The text carries the name of a type; what is stored is its id. For the
    /// fixtures both are the same — that saves a translation that proves
    /// nothing here.
    fn directory() -> Directory {
        Directory::new(
            [
                "character",
                "place",
                "species",
                "organization",
                "gender",
                "language",
                "articletype",
                "datatype",
                "abilities",
            ]
            .into_iter()
            .map(|n| (n.to_string(), n.to_string())),
        )
    }

    // Shorthands for the fixtures: the same as above, with the directory.
    fn from_text(text: &str) -> Parsed {
        super::from_text(text, &directory())
    }

    fn to_text(template: &Template) -> String {
        super::to_text(template, &directory())
    }

    /// Write and read back — the way the editor does.
    ///
    /// The ids do not stand in the text. Whoever reads back has to take them
    /// from the previous state; otherwise every round trip would get new
    /// ones, and the comparison would only prove two random numbers differ.
    fn round_trip(template: &Template) -> (Parsed, String) {
        let text = to_text(template);
        let mut g = from_text(&text);
        assign_block_ids(&mut g.blocks, &template.blocks);
        assign_field_ids(&mut g.properties, &template.properties);
        (g, text)
    }

    fn read_ok(text: &str) -> (Infobox, Vec<TemplateBlock>) {
        let g = from_text(text);
        assert!(g.errors.is_empty(), "unexpected errors: {:?}", g.errors);
        (g.infobox, g.blocks)
    }

    fn errors_of(text: &str) -> Vec<TextError> {
        from_text(text).errors
    }

    #[test]
    fn a_block_may_stand_on_top_and_start_folded() {
        let t = template("<article>\n<body>\n<block title=Sheet style=statblock top fold=folded>\n</block>\n<block title=Story>\n</block>\n<block title=Family bottom fold=off>\n</block>\n</body>\n</article>\n");
        assert!(t.blocks[0].place == BlockPlace::Top && t.blocks[0].fold == BlockFold::Folded);
        assert!(t.blocks[1].place == BlockPlace::Flow && t.blocks[1].fold == BlockFold::Open);
        assert_eq!((t.blocks[2].place, t.blocks[2].fold), (BlockPlace::Bottom, BlockFold::Off));
        let (back, text) = round_trip(&t);
        assert!(text.contains("<block title=Sheet style=statblock top fold=folded>"), "{text}");
        assert!(text.contains("<block title=Family bottom fold=off>"), "{text}");
        assert!(!errors_of("<article>\n<body>\n<block title=X fold=maybe>\n</block>\n</body>\n</article>\n").is_empty());
        assert_eq!(back.blocks, t.blocks);
        assert!(!errors_of("<article>\n<body>\n<block title=X top bottom>\n</block>\n</body>\n</article>\n").is_empty());
    }

    #[test]
    fn a_block_may_stand_at_the_side_as_wide_as_it_says() {
        let t = template("<article>\n<body>\n<block title=Timeline side width=380>\n</block>\n</body>\n</article>\n");
        assert_eq!((t.blocks[0].place, t.blocks[0].width), (BlockPlace::Side, 380));
        let (back, text) = round_trip(&t);
        assert!(text.contains("<block title=Timeline side width=380>"), "{text}");
        assert_eq!(back.blocks, t.blocks);
        assert!(!errors_of("<article>\n<body>\n<block title=X side width=wide>\n</block>\n</body>\n</article>\n").is_empty());
        assert!(!errors_of("<article>\n<body>\n<block title=X side top>\n</block>\n</body>\n</article>\n").is_empty());
    }

    #[test]
    fn who_may_make_documents_stands_on_the_article() {
        let t = template("<article view=players edit=gm create=players>\n<body>\n<block title=Log>\n</block>\n</body>\n</article>\n");
        assert_eq!(t.create, "players");
        let (back, text) = round_trip(&t);
        assert!(text.contains("<article view=players edit=gm create=players>"), "{text}");
        assert_eq!(back.create, "players");
    }

    #[test]
    fn a_block_title_may_stand_smaller() {
        let t = template("<article>\n<body>\n<block title=Story>\n</block>\n<block title=Notes heading=3>\n</block>\n</body>\n</article>\n");
        assert_eq!((t.blocks[0].heading, t.blocks[1].heading), (1, 3));
        let (back, text) = round_trip(&t);
        assert!(text.contains("<block title=Notes heading=3>") && text.contains("<block title=Story>"), "{text}");
        assert_eq!(back.blocks, t.blocks);
        assert!(!errors_of("<article>\n<body>\n<block title=X heading=5>\n</block>\n</body>\n</article>\n").is_empty());
    }

    /// A template from text — for round-trip checks.
    fn template(text: &str) -> Template {
        let g = from_text(text);
        assert!(g.errors.is_empty(), "{:?}", g.errors);
        Template {
            id: "x".into(),
            name: "X".into(),
            icon: String::new(),
            supertype: String::new(),
            group: false,
            rules: g.rules,
            description: String::new(),
            auth: g.auth,
            name_pattern: g.name_pattern,
            order: Vec::new(),
            sort: Vec::new(),
            create: g.create,
            page: String::new(),
            properties: g.properties,
            infobox: g.infobox,
            blocks: g.blocks,
        }
    }

    /// The properties on top of a text — which must read without errors.
    fn properties_of(text: &str) -> Vec<TemplateField> {
        let g = from_text(text);
        assert!(g.errors.is_empty(), "unexpected errors: {:?}", g.errors);
        g.properties
    }

    #[test]
    fn properties_of_every_kind() {
        let p = properties_of(
            r#"
---
Portrait: image 200x260
Species: @@species@@
Guilds: @@organization@@ multiple
Status: @@datatype@@
Languages: @@language@@ multiple adds
Age: number years
Born: date
Title: text
Motto: textarea
Mood: text = calm
---
"#,
        );
        assert_eq!(p.len(), 10);
        assert_eq!(p[0].label, "Portrait", "the name is what stands before the colon");
        assert_eq!(p[0].kind, FieldKind::Image { width: 200, height: 260, align: "center".into() });
        assert_eq!(
            p[2].kind,
            FieldKind::Reference { article_type: "organization".into(), multiple: true, parent: false, adds: false, count: String::new(), every: false, from: String::new() },
            "a bare `multiple` without = is a switch"
        );
        assert_eq!(p[3].kind, FieldKind::Reference { article_type: "datatype".into(), multiple: false, parent: false, adds: false, count: String::new(), every: false, from: String::new() });
        assert_eq!(
            p[4].kind,
            FieldKind::Reference { article_type: "language".into(), multiple: true, parent: false, adds: true, count: String::new(), every: false, from: String::new() },
            "`adds`: what it picks adds its numbers"
        );
        assert_eq!(p[5].kind, FieldKind::Number { unit: Some("years".into()) });
        assert_eq!(p[6].kind, FieldKind::WorldDate);
        assert_eq!(p[8].kind, FieldKind::Text { multiline: true });
        assert_eq!(p[9].kind, FieldKind::Text { multiline: false }, "without a tag it is text");
        assert_eq!(p[9].default, "calm", "and what stands there its default");
    }

    #[test]
    fn a_pick_may_carry_a_number() {
        let p = properties_of("---\nClasses: @@character@@ multiple with Level\nBag: @@datatype@@ multiple with \"How many\"\n---\n");
        assert_eq!(p[0].kind, FieldKind::Reference { article_type: "character".into(), multiple: true, parent: false, adds: false, count: "Level".into(), every: false, from: String::new() });
        let FieldKind::Reference { count, .. } = &p[1].kind else { panic!() };
        assert_eq!(count, "How many");
        let skills = template("---\nSkills: @@language@@ every with Proficiency\n---\n");
        assert!(matches!(&skills.properties[0].kind, FieldKind::Reference { every: true, multiple: true, count, .. } if count == "Proficiency"));
        assert!(to_text(&skills).contains("Skills: @@language@@ every with Proficiency\n"), "{}", to_text(&skills));
        assert!(errors_of("---\nSkills: @@language@@ every\n---\n")[0].text.contains("every needs a number"));
        let written = to_text(&template("---\nClasses: @@character@@ multiple with Level\n---\n"));
        assert!(written.contains("Classes: @@character@@ multiple with Level\n"), "{written}");
        assert!(errors_of("---\nClasses: @@character@@ multiple with\n---\n")[0].text.contains("with names"));
    }

    #[test]
    fn a_reference_may_show_what_points_here() {
        let next = template("---\nPrevious: @@character@@\nNext: @@character@@ from Previous\n---\n");
        assert!(matches!(&next.properties[1].kind, FieldKind::Reference { from, multiple: false, .. } if from == "Previous"));
        let written = to_text(&next);
        assert!(written.contains("Next: @@character@@ from Previous\n"), "{written}");
        assert!(errors_of("---\nNext: @@character@@ from\n---\n")[0].text.contains("from names"));
        assert!(errors_of("---\nNext: @@character@@ from Previous = Ada\n---\n")[0].text.contains("no default"));
    }

    #[test]
    fn a_group_is_taken_in_by_name() {
        let p = properties_of("---\nStats: group abilities\nabilities: group\n---\n");
        assert_eq!(p[0].kind, FieldKind::Group { group: "abilities".into() });
        assert_eq!(p[1].kind, FieldKind::Group { group: "abilities".into() }, "without a name: the property's own");
        let written = to_text(&template("---\nStats: group abilities\nabilities: group\n---\n"));
        assert!(written.contains("Stats: group abilities\nabilities: group\n"), "{written}");
        assert!(errors_of("---\nStats: group nothing\n---\n")[0].text.contains("no type “nothing”"));
        assert!(errors_of("---\nStats: group abilities = 3\n---\n")[0].text.contains("defaults in the group"));
        assert!(errors_of("---\nStats: group abilities view=gm\n---\n")[0].text.contains("said in the group"));
    }

    #[test]
    fn a_group_is_its_properties_and_a_layout() {
        let GroupText { properties: p, layout, errors: e, .. } = group_from_text("Strength: number\n\nTitle:\n", &directory());
        assert!(e.is_empty(), "{e:?}");
        assert!(layout.is_empty());
        assert_eq!(p.iter().map(|f| f.label.as_str()).collect::<Vec<_>>(), ["Strength", "Title"]);
        let group = Template { group: true, properties: p, ..template("") };
        assert_eq!(to_text(&group), "Strength: number\nTitle:\n");

        let GroupText { properties: p, layout, errors: e, .. } =
            group_from_text("---\nStrength: number\n---\n\n**Strength** %%Strength%%\n<abilities>\n", &directory());
        assert!(e.is_empty(), "{e:?}");
        assert_eq!(p.len(), 1);
        assert_eq!(layout, "**Strength** %%Strength%%\n<abilities>");

        let e = group_from_text("Strength: number\n<block>\nStrength: text\n", &directory()).errors;
        assert_eq!(e.iter().map(|e| e.line).collect::<Vec<_>>(), [2, 3]);
    }

    #[test]
    fn rules_stand_among_the_properties() {
        let t = template("---\nStrength: number\nmod(x) = floor((x - 10) / 2)\ncrit = 20\nMotto: = a = b\n---\n");
        assert_eq!(t.properties.iter().map(|f| f.label.as_str()).collect::<Vec<_>>(), ["Strength", "Motto"]);
        assert_eq!(
            t.rules,
            [
                Rule { name: "mod".into(), params: vec!["x".into()], formula: "floor((x - 10) / 2)".into() },
                Rule { name: "crit".into(), params: vec![], formula: "20".into() },
            ]
        );
        let text = to_text(&t);
        assert!(text.starts_with("---\nStrength: number\nMotto: text = a = b\nmod(x) = floor((x - 10) / 2)\ncrit = 20\n---\n"), "{text}");

        let g = group_from_text("Strength: number\ncheck(bonus) = 1d20 + bonus\n", &directory());
        assert!(g.errors.is_empty(), "{:?}", g.errors);
        assert_eq!(g.rules[0].params, ["bonus"]);
        let group = Template { group: true, properties: g.properties, rules: g.rules, ..template("") };
        assert_eq!(to_text(&group), "Strength: number\ncheck(bonus) = 1d20 + bonus\n");

        let wrong = |line: &str| errors_of(&format!("---\n{line}\n---\n"))[0].text.clone();
        assert!(wrong("floor(x) = x").contains("built in"));
        assert!(wrong("mod(x, x) = x").contains("twice"));
        assert!(wrong("mod(x = x").contains("not closed"));
        assert!(wrong("two words = 1").contains("one word"));
        assert!(wrong("crit =").contains("nothing after"));
        assert!(errors_of("---\ncrit = 20\nCrit = 19\n---\n")[0].text.contains("twice"));
    }

    #[test]
    fn a_property_may_have_a_default() {
        let p = properties_of("---\nAge: number years = 30\nSpecies: @@species@@ = Human\nMotto: = Never again\n---\n");
        assert_eq!(p[0].default, "30");
        assert_eq!(p[1].default, "Human", "a document of the type, by name");
        assert_eq!(p[2].kind, FieldKind::Text { multiline: false }, "no kind is text");
        assert_eq!(p[2].default, "Never again");
        let f = errors_of("---\nMood: calm\n---\n");
        assert!(f[0].text.contains("Mood: text = calm"), "a bare word is no default: {f:?}");
        let t = template("---\nAge: number years = 30\nMood: text = calm\nNotes:\n---\n");
        let text = to_text(&t);
        assert!(text.starts_with("---\nAge: number years = 30\nMood: text = calm\nNotes:\n---\n\n<article"), "{text}");
    }

    #[test]
    fn the_infobox_shows_properties() {
        let g = from_text(
            "---\nTitle:\nSpecies: @@species@@\n---\n<article view=all edit=all>\n  <name>\n  <body>\n    <infobox align=right>\n      <text bind=Title>\n      <select bind=Species label=Kind>\n    </infobox>\n  </body>\n</article>\n",
        );
        assert!(g.errors.is_empty(), "{:?}", g.errors);
        let shown: Vec<(&str, &str, &str)> = g
            .infobox
            .show
            .iter()
            .map(|s| (s.element.as_str(), s.property.as_str(), s.label.as_str()))
            .collect();
        assert_eq!(shown, [("text", "Title", ""), ("select", "Species", "Kind")]);
        let uses: Vec<(Option<&str>, &str, usize)> =
            g.uses.iter().map(|u| (u.element.as_deref(), u.property.as_str(), u.line)).collect();
        assert_eq!(uses, [(Some("text"), "Title", 9), (Some("select"), "Species", 10)]);

        let f = errors_of("<infobox>\n  <text label=A>\n</infobox>\n");
        assert!(f[0].text.contains("bind="), "an element in the infobox binds: {f:?}");
    }

    #[test]
    fn reading_in_the_text_is_noticed() {
        let g = from_text("<block title=T>\n  He is %%Age%% years old.\n  And %%Mood%%.\n</block>\n");
        let uses: Vec<(Option<&str>, &str, usize)> =
            g.uses.iter().map(|u| (u.element.as_deref(), u.property.as_str(), u.line)).collect();
        assert_eq!(uses, [(None, "Age", 2), (None, "Mood", 3)]);
        assert!(g.blocks[0].parts[0].text.contains("%%Age%%"), "the text keeps it");
    }

    #[test]
    fn blocks_carry_their_text_between_the_tags() {
        let (_, b) = read_ok(
            r#"
<infobox align=none>
</infobox>

<block key=history title="History">
  One paragraph.

  Another one.
</block>

<block title="The truth" style=box view="class:rogue OR gm">
  For the few.
</block>
"#,
        );
        assert_eq!(b.len(), 2);
        assert_eq!(b[0].parts[0].text, "One paragraph.\n\nAnother one.");
        assert_eq!(b[1].style, BlockStyle::Box);
        assert_eq!(
            b[1].parts[0].view, "class:rogue OR gm",
            "a policy with spaces goes in quotes and stays whole"
        );
    }

    #[test]
    fn an_inner_block_cuts_a_block_into_parts() {
        // The core: a region with access of its own becomes a part of its
        // own — and therefore later its own layer with its own rule.
        let (_, b) = read_ok(
            r#"
<block title="History">
  Everyone knows that.

  <block view=gm edit=gm>
    Only the GM knows that.
  </block>

  And everyone again.
</block>
"#,
        );
        assert_eq!(b.len(), 1, "it stays one block");
        let t = &b[0].parts;
        assert_eq!(t.len(), 3, "but three parts: before, guarded, after");

        assert_eq!(t[0].key, b[0].key, "the first carries the block's id");
        assert_eq!(t[1].key, format!("{}-2", b[0].key), "the others count up");
        assert_eq!(t[0].text, "Everyone knows that.");
        assert_eq!(t[0].view, "all", "without access of its own everyone may do everything");
        assert_eq!(t[0].edit, "all");

        assert_eq!(t[1].text, "Only the GM knows that.");
        assert_eq!(t[1].view, "gm");
        assert_eq!(t[1].edit, "gm");

        assert_eq!(t[2].key, format!("{}-3", b[0].key));
        assert_eq!(t[2].text, "And everyone again.");
        assert_eq!(t[2].view, "all", "afterwards the default applies again");
    }

    #[test]
    fn an_inner_block_survives_the_round_trip() {
        let t = template("<block title=\"K\">\n  Open.\n  <block view=gm edit=gm>\n    Secret.\n  </block>\n  Open again.\n</block>\n");
        let (g, text) = round_trip(&t);
        assert!(g.errors.is_empty(), "{:?}", g.errors);
        assert_eq!(g.blocks, t.blocks, "written and parsed again is the same");
        assert_eq!(g.auth, t.auth);
        assert!(!text.contains("auth"), "{text}");
    }

    #[test]
    fn without_access_everyone_may_everything() {
        // The default is open. A silent restriction would be worse:
        // whoever wants to protect something writes it down.
        let g = from_text("---\nA:\n---\n<block title=\"T\">\n  Text.\n</block>");
        assert!(g.errors.is_empty(), "{:?}", g.errors);
        assert_eq!(g.properties[0].access, None, "a property takes the article's");
        assert_eq!(g.blocks[0].parts[0].view, "all");
        assert_eq!(g.blocks[0].parts[0].edit, "all");
    }

    #[test]
    fn the_deepest_access_wins() {
        let g = from_text(
            r#"
<article view=players edit=gm>
  <name>
  <body>
    <block title="History">
      The players see this.
      <block view=gm>
        Not this.
        <block view=all>
          But everyone this.
        </block>
      </block>
      And this again.
    </block>
    <block title="Notes" view=gm edit=gm>
      Only the GM.
    </block>
  </body>
</article>
"#,
        );
        assert!(g.errors.is_empty(), "{:?}", g.errors);
        let b = &g.blocks;
        assert_eq!(
            g.auth,
            Some(Access {
                view: "players".into(),
                edit: "gm".into()
            })
        );
        let views: Vec<&str> = b[0].parts.iter().map(|p| p.view.as_str()).collect();
        assert_eq!(views, ["players", "gm", "all", "players"]);
        assert_eq!(b[0].parts[1].edit, "gm", "what an inner block leaves out comes from outside");
        assert_eq!(b[1].parts[0].view, "gm", "a block's own access overrides the article's");
    }

    #[test]
    fn the_articles_access_survives_the_round_trip() {
        let t = template(
            "<article view=players edit=gm>\n  <name>\n  <body>\n    <block title=\"T\">\n      Open.\n      <block view=gm edit=gm>\n        Secret.\n      </block>\n    </block>\n  </body>\n</article>\n",
        );
        let (g, text) = round_trip(&t);
        assert!(text.starts_with("<article view=players edit=gm>"), "{text}");
        assert!(g.errors.is_empty(), "{:?}\n{text}", g.errors);
        assert_eq!(g.auth, t.auth);
        assert_eq!(g.blocks, t.blocks);
    }

    #[test]
    fn access_for_a_whole_block_stands_on_its_tag() {
        let t = template(
            "<block title=\"Secret\" view=gm edit=gm>\n  First paragraph.\n\n  Second paragraph.\n</block>\n\n<block title=\"Open\">\n  For everyone.\n</block>\n",
        );
        let text = to_text(&t);
        assert!(
            text.contains("<block title=Secret view=gm edit=gm>"),
            "the access stands on the tag:\n{text}"
        );
        assert_eq!(text.matches("<block").count(), 2, "no inner block:\n{text}");
        assert!(text.contains("Second paragraph."), "{text}");

        let (g, _) = round_trip(&t);
        assert!(g.errors.is_empty(), "{:?}\n{text}", g.errors);
        assert_eq!(g.blocks, t.blocks, "{text}");
    }

    #[test]
    fn an_open_block_is_caught() {
        let f = errors_of("<block title=A>\n  Text.\n  <block view=gm>\n  Secret.\n</block>\n");
        assert_eq!(f.len(), 1, "{f:?}");
        assert_eq!(f[0].line, 1, "the outer one stays open: {f:?}");
        let g = from_text("<block title=A>\n  Text.\n");
        assert_eq!(g.blocks[0].parts[0].text, "Text.", "what was understood is kept");
    }

    #[test]
    fn an_inner_block_has_no_title() {
        let f = errors_of("<block title=A>\n  <block title=B view=gm>\n  </block>\n</block>\n");
        assert_eq!(f.len(), 1, "{f:?}");
        assert!(f[0].text.contains("no title"), "{}", f[0].text);
    }

    #[test]
    fn access_on_a_block_survives_the_round_trip() {
        let t = template(
            "<article view=all edit=all>\n  <name>\n  <body>\n    <block title=\"Half\" view=players edit=gm>\n      All players.\n      <block view=gm edit=gm>\n        Only the game master.\n      </block>\n    </block>\n  </body>\n</article>\n",
        );
        assert_eq!(t.blocks[0].parts.len(), 2);
        assert_eq!(t.blocks[0].parts[0].view, "players");
        assert_eq!(t.blocks[0].parts[1].view, "gm");

        let (g, text) = round_trip(&t);
        assert!(g.errors.is_empty(), "{:?}\n{text}", g.errors);
        assert_eq!(g.blocks, t.blocks, "{text}");
    }

    #[test]
    fn an_image_in_the_text_stays_in_the_text() {
        // That is the point of tags: `<image>` may stand in the middle of a
        // paragraph and is not mistaken there for a field of the infobox.
        let (_, b) = read_ok(
            "<block key=k title=\"K\">\n  Before.\n  <image src=map align=right width=300>\n  After.\n</block>\n",
        );
        assert_eq!(
            b[0].parts[0].text,
            "Before.\n<image src=map align=right width=300>\nAfter."
        );
    }

    #[test]
    fn the_id_does_not_come_from_the_title() {
        // The block key used to come from the title — so every rename changed
        // it, and the layers of existing articles hung on a name that no
        // longer existed.
        let (_, b) = read_ok("<block title=\"The truth\">\n  Text.\n</block>");
        assert!(!b[0].key.is_empty(), "it has an id");
        assert_ne!(b[0].key, "the-truth", "but not the title");
        assert_eq!(b[0].parts[0].key, b[0].key, "the first part carries it");

        // And parsing the same thing twice gives two different ones: the id
        // belongs to the block, not to its title.
        let (_, again) = read_ok("<block title=\"The truth\">\n  Text.\n</block>");
        assert_ne!(b[0].key, again[0].key);
    }

    #[test]
    fn errors_name_their_line() {
        let errors = errors_of("---\nX: choice\nY: @@@@\n---\n<infobox align=slanted>\n</infobox>\n");
        assert_eq!(errors[0].line, 2);
        assert!(errors[0].text.contains("choice"), "{:?}", errors[0]);
        assert!(errors[0].text.contains("number"), "and names the possible ones");
        assert_eq!(errors[1].line, 3, "all errors at once, not one");
        assert!(errors[1].text.contains("type"), "{:?}", errors[1]);
        assert_eq!(errors[2].line, 5);
        assert!(errors[2].text.contains("align="), "{:?}", errors[2]);
    }

    #[test]
    fn equal_titles_are_still_two_blocks() {
        // Two blocks may share a name — they are still two, and each carries
        // its own layers. The key used to be the same both times.
        let (_, b) = read_ok(
            "<block title=\"Notes\">\n  One.\n</block>\n\n<block title=\"Notes\">\n  Two.\n</block>",
        );
        assert_eq!(b.len(), 2);
        assert_ne!(b[0].key, b[1].key);
    }

    #[test]
    fn comments_are_skipped() {
        let (b, _) = read_ok("<!-- Note to self -->\n<infobox align=top>\n  <text bind=A>\n</infobox>");
        assert_eq!(b.align, "top");
        assert_eq!(b.show.len(), 1);
    }

    #[test]
    fn the_round_trip_changes_nothing() {
        // The real test: what is written and read back again has to be the
        // same — otherwise saving silently loses something. A template that
        // holds every field kind and every block style, including those
        // nobody would create on their own.
        let t = template(
            "---\nPortrait: image 200x260 left\nTitle:\nMotto: textarea = Never again\nAge: number years = 30\nBorn: date\nStatus: @@datatype@@\nLanguages: @@language@@ multiple\nLies in: parent\nHome: @@place@@ multiple\nSecret: text view=gm edit=gm\n---\n\n<article view=players edit=gm>\n    <name=\"Entry %%count%%\">\n    <body>\n      <infobox align=right width=320>\n        <image bind=Portrait>\n        <text bind=Title>\n        <select bind=Status label=\"How\">\n      </infobox>\n      <block title=\"Open\">\n        One paragraph.\n\n        Another with a @@link@@.\n      </block>\n      <block title=\"Box\" style=box view=gm edit=gm>\n        Secret.\n      </block>\n      <block title=\"Quote\" style=quote>\n        Open at first.\n        <block view=gm edit=gm>\n          Then no longer.\n        </block>\n        <image size=300x200 align=right view=gm edit=gm>\n        <list type=place columns=2 view=gm edit=gm>\n      </block>\n    </body>\n</article>\n",
        );

        let (g, text) = round_trip(&t);
        assert!(g.errors.is_empty(), "{:?}\n{text}", g.errors);
        assert_eq!(g.infobox.align, t.infobox.align, "{text}");
        assert_eq!(g.infobox.width, t.infobox.width, "{text}");
        assert_eq!(g.infobox.show, t.infobox.show, "{text}");
        assert_eq!(g.properties, t.properties, "{text}");
        assert_eq!(g.blocks, t.blocks, "{text}");
        assert_eq!(g.auth, t.auth, "{text}");
        assert_eq!(g.name_pattern, t.name_pattern, "{text}");
        assert!(
            text.contains("\n      <image size=300x200 align=right view=gm edit=gm>\n"),
            "an element keeps its access on itself:\n{text}"
        );
    }

    #[test]
    fn a_property_may_have_access_of_its_own() {
        let t = template("---\nTitle:\nSecret: text view=gm edit=gm\nPortrait: image 100x100 view=gm\n---\n<article view=players edit=gm>\n  <name>\n  <body>\n  </body>\n</article>\n");
        assert_eq!(t.properties[0].access, None);
        let secret = t.properties[1].access.as_ref().unwrap();
        assert_eq!((secret.view.as_str(), secret.edit.as_str()), ("gm", "gm"));
        assert_eq!(t.properties[2].access.as_ref().unwrap().edit, "gm", "from the article");

        let (g, text) = round_trip(&t);
        assert!(g.errors.is_empty(), "{:?}\n{text}", g.errors);
        assert!(text.contains("Secret: text view=gm edit=gm"), "{text}");
        assert_eq!(g.properties, t.properties, "{text}");

        let f = errors_of("---\nA:\n---\n<infobox view=gm>\n  <text bind=A view=gm>\n</infobox>\n");
        assert_eq!(f.len(), 1, "who sees a property says the property — the infobox may say its own: {f:?}");
    }

    #[test]
    fn the_infobox_may_have_access_of_its_own() {
        let t = template("---\nA:\n---\n<article view=players edit=gm>\n  <name>\n  <body>\n    <infobox align=right width=auto view=players edit=players>\n      <text bind=A>\n    </infobox>\n  </body>\n</article>\n");
        assert_eq!(t.infobox.access.as_ref().map(|a| a.edit.as_str()), Some("players"));
        let (g, text) = round_trip(&t);
        assert!(g.errors.is_empty(), "{:?}\n{text}", g.errors);
        assert!(text.contains("<infobox align=right width=auto view=players edit=players>"), "{text}");
        assert_eq!(g.infobox, t.infobox, "{text}");
        let same = template("---\nA:\n---\n<article view=players edit=gm>\n  <name>\n  <body>\n    <infobox view=players edit=gm>\n      <text bind=A>\n    </infobox>\n  </body>\n</article>\n");
        assert_eq!(same.infobox.access, None, "the article's access is none of its own");
    }

    #[test]
    fn an_element_with_access_is_a_part_of_its_own() {
        let (_, b) = read_ok(
            "<block title=K>\n  Before.\n  <image size=300x200 view=gm edit=gm>\n  After.\n</block>\n",
        );
        let parts: Vec<(&str, &str)> = b[0].parts.iter().map(|p| (p.text.as_str(), p.view.as_str())).collect();
        assert_eq!(
            parts,
            [("Before.", "all"), ("<image size=300x200>", "gm"), ("After.", "all")]
        );
    }

    #[test]
    fn every_property_gets_its_own_id() {
        // Nobody writes them down, and nobody gets to see them. The value in
        // the article sits under them — so they have to be unique and must
        // not hang on the name.
        let p = properties_of("---\nBorn: date\nTrue goal:\nPortrait: image\n---\n");
        let id: Vec<&str> = p.iter().map(|f| f.key.as_str()).collect();
        assert_eq!(id.len(), 3);
        assert!(id.iter().all(|k| !k.is_empty()));
        assert_ne!(id[0], id[1]);
        assert!(!id.contains(&"born"), "not from the name");
    }

    #[test]
    fn an_image_is_the_same_everywhere() {
        // The same tag, the same details — as a property on top or in
        // flowing text.
        let g = from_text("---\nCrest: image 120x120 left\n---\n<block title=\"T\">\n  <image size=300x200 align=right>\n</block>");
        assert!(g.errors.is_empty(), "{:?}", g.errors);
        assert_eq!(g.properties[0].kind, FieldKind::Image { width: 120, height: 120, align: "left".into() });
        assert!(
            g.blocks[0].parts[0].text.contains("align=right"),
            "in the text it stays as a tag: {}",
            g.blocks[0].parts[0].text
        );
    }

    #[test]
    fn width_and_height_work_separately_too() {
        let p = properties_of("---\nP: image 200x260\n---\n");
        assert_eq!(p[0].kind, FieldKind::Image { width: 200, height: 260, align: "center".into() });
    }

    #[test]
    fn the_infobox_width_may_be_a_number() {
        let (b, _) = read_ok("<infobox width=320>\n  <text bind=A>\n</infobox>");
        assert_eq!(b.width, "320");
        let errors = errors_of("<infobox width=huge>\n  <text bind=A>\n</infobox>");
        assert!(errors[0].text.contains("width="), "{:?}", errors[0]);
    }

    #[test]
    fn no_id_stands_in_the_text() {
        // The id belongs to storage. Standing in the text, someone would
        // have to read an id they never chose — and after a rename it would
        // carry the old name.
        let t = template("---\nPortrait: image 200x260\nName:\n---\n<infobox align=right width=auto>\n  <image bind=Portrait>\n  <text bind=Name>\n</infobox>\n\n<block title=\"History\">\n  Text.\n</block>\n");
        let back = to_text(&t);

        assert!(!back.contains("key="), "no key= anywhere:\n{back}");
        assert!(back.contains("Portrait: image 200x260"), "{back}");
        assert!(back.contains("<text bind=Name>"), "{back}");
        assert!(back.contains("<block title=History"), "{back}");
    }

    #[test]
    fn a_label_stands_only_where_written() {
        let (b, _) = read_ok("<infobox>\n  <image bind=Portrait>\n  <text bind=Title label=\"Called\">\n</infobox>");
        assert_eq!(b.show[0].label, "", "not invented");
        assert_eq!(b.show[1].label, "Called");
    }

    #[test]
    fn umlauts_are_transliterated_in_a_key() {
        assert_eq!(key_from("Größe"), "groesse");
        assert_eq!(key_from("True goal"), "true-goal");
        assert_eq!(key_from("  ---  "), "");
    }

    #[test]
    fn a_deleted_skeleton_is_caught() {
        // The skeleton can be changed but not deleted. For the editor to put
        // it back, it has to learn that it is missing.
        let intact = "<article view=all edit=all>\n    <name>\n    <body>\n    </body>\n</article>\n";
        assert!(from_text(intact).skeleton_intact);

        for (what, text) in [
            (
                "without <name>",
                "<article view=all edit=all>\n    <body>\n    </body>\n</article>\n",
            ),
            (
                "without <body>",
                "<article view=all edit=all>\n    <name>\n</article>\n",
            ),
            (
                "<name> twice",
                "<article view=all edit=all>\n    <name>\n    <name>\n    <body>\n    </body>\n</article>\n",
            ),
            ("gone entirely", "<block title=\"T\">\n  Text.\n</block>\n"),
        ] {
            assert!(!from_text(text).skeleton_intact, "{what} should be noticed");
        }
    }

    #[test]
    fn a_half_typed_character_does_not_eat_the_line_below() {
        // Whoever wants to insert something in the <body> types a `<` first.
        // It must not catch the `>` of the next line: otherwise `</body>`
        // would be used up, the skeleton would count as deleted — and the
        // editor would write the text afresh, including the character just
        // typed.
        for half in ["<", "<bl", "<block title=", "<!-- not finished yet"] {
            // On a line of its own …
            let text = format!(
                "<article view=all edit=all>\n    <name>\n    <body>\n      {half}\n    </body>\n</article>\n"
            );
            assert!(
                from_text(&text).skeleton_intact,
                "“{half}” should not damage the skeleton"
            );

            // … and right before the end tag, when someone starts typing
            // before breaking the line.
            let text = format!(
                "<article view=all edit=all>\n    <name>\n    <body>\n      {half}    </body>\n</article>\n"
            );
            assert!(
                from_text(&text).skeleton_intact,
                "“{half}” before </body> should not damage the skeleton"
            );
        }
    }

    #[test]
    fn a_closed_note_may_span_lines() {
        let g = from_text(
            "<article view=all edit=all>\n    <name>\n    <body>\n      <!-- first\n           then -->\n      <block title=\"T\">\n        Text.\n      </block>\n    </body>\n</article>\n",
        );
        assert!(g.errors.is_empty(), "{:?}", g.errors);
        assert!(g.skeleton_intact);
        assert_eq!(g.blocks.len(), 1);
    }

    #[test]
    fn a_half_typed_region_is_not_damage() {
        // Whoever types a region has it open for a moment. That must not
        // make the text be written afresh straight away — otherwise one would
        // be typing against the editor.
        let text = "<article view=all edit=all>\n    <name>\n    <body>\n      <block title=T>\n        <block view=gm edit=gm>\n      </block>\n    </body>\n</article>\n";
        assert!(from_text(text).skeleton_intact);
    }

    #[test]
    fn whoever_follows_the_article_gets_no_access_of_its_own() {
        // The counter-check for the interface: if someone changes the
        // article's access, <article> changes — and everything that matched
        // it before still matches it afterwards. Then only what really
        // differs may say so in the text.
        let mut t = template(
            "<article view=players edit=gm>\n    <name>\n    <body>\n      <block title=\"History\">\n        Open.\n        <block view=gm edit=gm>\n          Secret.\n        </block>\n      </block>\n    </body>\n</article>\n",
        );

        let fresh = Access {
            view: "all".into(),
            edit: "gm".into(),
        };
        t.auth = Some(fresh.clone());
        t.blocks[0].parts[0].view = fresh.view.clone();

        let text = to_text(&t);
        assert!(text.starts_with("<article view=all edit=gm>\n"), "{text}");
        // One: the part that really differs.
        assert_eq!(text.matches(" view=").count(), 2, "{text}");
        assert_eq!(from_text(&text).auth, t.auth);
    }

    #[test]
    fn a_reference_picks_documents_of_a_data_type() {
        // The values then stand there as documents and can be maintained
        // without touching every template that uses them.
        let t = template("---\nGender: @@gender@@\n---\n");
        assert_eq!(
            t.properties[0].kind,
            FieldKind::Reference { article_type: "gender".into(), multiple: false, parent: false, adds: false, count: String::new(), every: false, from: String::new() }
        );
        let text = to_text(&t);
        assert!(text.contains("Gender: @@gender@@"), "{text}");
    }

    #[test]
    fn a_relation_names_its_roles_type() {
        let t = template("---\nRelations: @@character@@ as @@datatype@@ view=gm edit=gm\n---\n");
        assert_eq!(
            t.properties[0].kind,
            FieldKind::Relation { article_type: "character".into(), role_type: "datatype".into() }
        );
        assert_eq!(t.properties[0].access.as_ref().map(|a| a.view.as_str()), Some("gm"));
        let text = to_text(&t);
        assert!(text.contains("Relations: @@character@@ as @@datatype@@ view=gm edit=gm"), "{text}");

        for (line, says) in [
            ("X: @@character@@ as", "type of the roles"),
            ("X: @@character@@ as @@nothing@@", "There is no type"),
            ("X: @@character@@ as @@datatype@@ multiple", "always holds several"),
            ("X: @@character@@ as @@datatype@@ = Mother", "no default"),
            ("X: @@character@@ ascending", "means nothing after a type"),
        ] {
            let f = errors_of(&format!("---\n{line}\n---\n"));
            assert_eq!(f.len(), 1, "{line}: {f:?}");
            assert!(f[0].text.contains(says), "{line}: {}", f[0].text);
        }
    }

    #[test]
    fn the_parent_says_its_type_itself() {
        let front = |line: &str| format!("---\n{line}\n---\n");

        // Bare: the root of the own type tree.
        let i = template(&front("Lies in: parent"));
        assert_eq!(
            i.properties[0].kind,
            FieldKind::Reference { article_type: String::new(), multiple: false, parent: true, adds: false, count: String::new(), every: false, from: String::new() }
        );
        // Or a type of its own, when the tree crosses type borders.
        let j = template(&front("Belongs to: @@place@@ parent"));
        assert_eq!(
            j.properties[0].kind,
            FieldKind::Reference { article_type: "place".into(), multiple: false, parent: true, adds: false, count: String::new(), every: false, from: String::new() }
        );

        // Both come back as they were written.
        for (t, expected) in [(i, "Lies in: parent"), (j, "Belongs to: @@place@@ parent")] {
            let text = to_text(&t);
            assert!(text.contains(expected), "{text}");
        }

        // A parent is exactly one document.
        let f = errors_of(&front("X: @@place@@ parent multiple"));
        assert_eq!(f.len(), 1, "{f:?}");
        assert!(f[0].text.contains("exactly one"), "{}", f[0].text);
        let f = errors_of(&front("X: parent multiple"));
        assert_eq!(f.len(), 1, "{f:?}");
    }

    #[test]
    fn the_name_pattern_survives_the_round_trip() {
        // The <name> carries its value itself — `<name pattern="…">` said
        // the same thing twice.
        let t = template(
            "<article view=all edit=all>\n    <name=\"Session log %%count%%\">\n    <body>\n    </body>\n</article>\n",
        );
        assert_eq!(t.name_pattern, "Session log %%count%%");
        let text = to_text(&t);
        assert!(text.contains("<name=\"Session log %%count%%\">"), "{text}");
        assert_eq!(from_text(&text).name_pattern, t.name_pattern);

        // Without a pattern the tag stays plain.
        let plain = template(
            "<article view=all edit=all>\n    <name>\n    <body>\n    </body>\n</article>\n",
        );
        assert_eq!(plain.name_pattern, "%%input%%");
        assert!(to_text(&plain).contains("<name>\n"));
    }

    #[test]
    fn a_pattern_without_a_placeholder_is_caught() {
        // Otherwise every article of the type would share a name.
        let g = from_text(
            "<article view=all edit=all>\n    <name=\"Session log\">\n    <body>\n    </body>\n</article>\n",
        );
        assert_eq!(g.errors.len(), 1, "{:?}", g.errors);
        assert_eq!(g.errors[0].line, 2);

        // And a mistyped placeholder too — it would otherwise stand in every
        // name.
        let g = from_text(
            "<article view=all edit=all>\n    <name=\"Log %%number%%\">\n    <body>\n    </body>\n</article>\n",
        );
        assert_eq!(g.errors.len(), 1, "{:?}", g.errors);
        assert!(g.errors[0].text.contains("%%number%%"), "{}", g.errors[0].text);
        // It is parsed anyway: the preview should keep up.
        assert_eq!(g.name_pattern, "Log %%number%%");
    }

    #[test]
    fn a_pattern_with_input_and_count_is_allowed() {
        assert!(check_pattern("%%input%%").is_ok());
        assert!(check_pattern("Item %%input%%").is_ok());
        assert!(check_pattern("Session log %%count%%").is_ok());
        assert!(check_pattern("%%count%% — %%input%%").is_ok());
        assert!(check_pattern("Session log %%count").is_err());
    }

    #[test]
    fn the_skeleton_is_always_there() {
        // Every article has a name and a body. That should be visible and
        // not have to be guessed — even for a fresh template.
        let t = Template {
            id: "x".into(),
            name: "X".into(),
            icon: String::new(),
            supertype: String::new(),
            group: false,
            rules: Vec::new(),
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
        assert_eq!(
            to_text(&t),
            "<article view=all edit=all>\n  <name>\n  <body>\n  </body>\n</article>\n"
        );
    }

    #[test]
    fn the_skeleton_survives_the_round_trip() {
        let t = template(
            "<article view=players edit=gm>\n    <name>\n    <body>\n      <block title=\"T\">\n        Text.\n      </block>\n    </body>\n</article>\n",
        );
        let (g, text) = round_trip(&t);
        assert!(g.errors.is_empty(), "{:?}\n{text}", g.errors);
        assert_eq!(
            t.auth,
            Some(Access {
                view: "players".into(),
                edit: "gm".into()
            })
        );
        assert_eq!(g.auth, t.auth, "{text}");
        assert_eq!(g.blocks, t.blocks, "{text}");
    }

    #[test]
    fn an_empty_block_takes_the_articles_access() {
        let (_, blocks) = read_ok("<article view=players edit=gm>\n  <name>\n  <body>\n    <block title=Empty>\n    </block>\n  </body>\n</article>\n");
        let part = &blocks[0].parts[0];
        assert_eq!((part.view.as_str(), part.edit.as_str()), ("players", "gm"));
    }

    #[test]
    fn an_empty_infobox_is_not_written() {
        let t = Template {
            id: "x".into(),
            name: "X".into(),
            icon: String::new(),
            supertype: String::new(),
            group: false,
            rules: Vec::new(),
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
        assert!(
            !to_text(&t).contains("<infobox"),
            "no box that shows nothing: {}",
            to_text(&t)
        );
    }

    #[test]
    fn a_broken_line_does_not_stop_the_rest() {
        // While typing, a half-finished line is the normal case. What stands
        // beside it must still arrive — otherwise the preview freezes.
        let g = from_text("---\nGood:\nBroken: nonsense\n---\n<block title=\"There\">\n  Text.\n</block>");
        assert_eq!(g.properties.len(), 1, "the good property arrives");
        assert_eq!(g.blocks.len(), 1, "and so does the block");
        assert_eq!(g.errors.len(), 1);
    }

    #[test]
    fn empty_text_gives_an_empty_template() {
        let g = from_text("");
        assert_eq!(g.infobox.align, "none");
        assert!(g.infobox.show.is_empty());
        assert!(g.properties.is_empty());
        assert!(g.blocks.is_empty());
    }
}
