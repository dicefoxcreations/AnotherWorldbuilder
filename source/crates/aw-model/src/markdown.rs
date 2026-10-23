//! Splitting Markdown — into a tree, not into HTML.
//!
//! The content of a text block is Markdown. But it is never rendered as an
//! HTML string: Rust splits it here into a small tree, the frontend builds
//! React elements from it. So there is no `dangerouslySetInnerHTML`
//! anywhere — and therefore no gap through which a foreign world author could
//! get script into the application.
//!
//! The scope is small and closed: headings, bold, italic, strikethrough,
//! lists, quote, code, rule, links and `@@Type:Article@@` as a link to a document
//! (see [`crate::link`]). Two tags
//! are interpreted: `<image>` and `<list>`.
//! **Raw HTML is shown as text, never executed.**

use pulldown_cmark::{Event, HeadingLevel, Options, Parser, Tag};
use serde::{Deserialize, Serialize};

/// A node in the rendered tree.
///
/// Deliberately kept flat: what the frontend cannot draw should not come
/// into being here in the first place.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum Node {
    Paragraph { children: Vec<Node> },
    Heading { level: u8, children: Vec<Node> },
    List { ordered: bool, items: Vec<Vec<Node>> },
    Quote { children: Vec<Node> },
    CodeBlock { language: String, text: String },
    Rule,
    /// A Markdown table: `| a | b |` lines with a `|---|` line under the head.
    /// `align` per column: `left`, `center`, `right` or empty.
    Table { align: Vec<String>, head: Vec<Vec<Node>>, rows: Vec<Vec<Vec<Node>>> },

    Text { text: String },
    Strong { children: Vec<Node> },
    Emphasis { children: Vec<Node> },
    Strikethrough { children: Vec<Node> },
    Code { text: String },
    SoftBreak,
    /// An outward link. The frontend decides whether to open it.
    Link { target: String, children: Vec<Node> },
    /// `^2f9e17` closing a line — a place in the text that a link leads to
    /// (`@@#^2f9e17|…@@`, see [`crate::link`]). Not shown.
    Anchor { id: String },
    /// `@@Type:Article@@` — the link to another article.
    ///
    /// It is only resolved when drawing: only there is it known which
    /// articles this reader could decrypt at all.
    Wikilink {
        target: String,
        text: String,
        /// The target's type as written — empty when the link does not say.
        #[serde(default)]
        of: String,
        /// A property of the target (`#Status`) — then its value is shown.
        #[serde(default)]
        property: String,
    },
    /// `<image …>` — the same tag as in the infobox.
    ///
    /// In flowing text the paragraph runs past it for `left` and `right`.
    ///
    /// The only tag interpreted in flowing text. Everything else stays
    /// text; see [`read_tag`].
    Image {
        /// The image: `img:…` from the gallery.
        src: String,
        /// `left`, `center` or `right` — where it stands.
        align: String,
        /// Whether the text runs past it — left or right only; a centred
        /// image always stands on its own. `wrap=no` turns it off.
        wrap: bool,
        width: u16,
        caption: String,
        /// A stand-in: the part of the document it stands for. Shown only
        /// to whoever cannot open that part — the real image stands there.
        #[serde(default, rename = "for")]
        stands_for: String,
    },
    /// `<list type=Place …>` — the documents of a type, drawn where it stands.
    ///
    /// Rust only reads what is asked for; which documents that is, the
    /// display decides — only there is it known what this reader can decrypt.
    /// The type goes by its name, like everywhere in text.
    Listing {
        /// Name of the type; its subtypes count too. Empty: every document.
        of: String,
        /// `list`, `cards`, `table` or `tree` — the last along the parent axis.
        style: String,
        /// 1 to 3 — for the alphabetical list.
        columns: u8,
        /// What a card shows besides the name: field labels, or `image`.
        show: Vec<String>,
        /// Only documents whose field has this value: `Role=NPC`. An empty
        /// value — `Lies in=` — means the field is empty: the top level;
        /// `Owner=*` that it has any value.
        filter: Vec<(String, String)>,
        /// Only documents below this one in the tree, by name.
        below: String,
        /// The documents a property of this document picks — `from=Equipment`;
        /// whoever may change the document adds and removes them in the list.
        /// A path goes on through the picks: `from=Classes.Features`.
        #[serde(default)]
        from: String,
        /// A formula each listed document must come to something other than
        /// 0 for — reckoned inside the pick: `when="From level <= Level"`.
        #[serde(default, skip_serializing_if = "String::is_empty")]
        when: String,
        /// A property its reader filters by — `filter=Role`: a button for
        /// each of its values above the list.
        #[serde(default, skip_serializing_if = "String::is_empty")]
        choose: String,
        /// The order it starts in, and goes back to — `sort="Role desc, name"`:
        /// properties by name, `desc` for the other way round. Empty: the
        /// type's own order.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        sort: Vec<String>,
        /// Counts instead of listing — `count=Killer`: a row for each value of
        /// the property, and how many documents have it, most first.
        #[serde(default, skip_serializing_if = "String::is_empty")]
        count: String,
        /// With `count`: a column for each value of this property too —
        /// `by=Kind`: how many of them are bosses, how many minions.
        #[serde(default, skip_serializing_if = "String::is_empty")]
        by: String,
        /// On a list of what names this document (`where="Owner=self"`):
        /// `new` — ＋ makes one, naming this document, and × deletes it; else
        /// ＋ picks what is there and × lets go of it.
        #[serde(default, skip_serializing_if = "String::is_empty")]
        add: String,
    },
    /// `<timeline type="Event, Session">` — what happened when: every dated
    /// property of the documents of these types, along the world's calendar.
    Timeline {
        /// Names of the types; their subtypes count too. Empty: every document.
        of: Vec<String>,
        /// Only these date properties, by name — empty for all of them.
        show: Vec<String>,
        /// Only what names this document — `self` for the one shown — or is it.
        around: String,
        /// A date property that ends what another begins — `until=End`: a
        /// span, not an entry of its own.
        #[serde(default)]
        until: String,
        /// Which part's text an entry shows, by title — the first of these a
        /// document has: `text="Summary, Course"`. Empty: its first text;
        /// `none`: no text.
        #[serde(default)]
        text: Vec<String>,
    },
    /// `<calendar type="Session, Event">` — a month of the world's
    /// calendar, its moons, and what of these types happened on its days.
    Calendar {
        /// Names of the types; their subtypes count too. Empty: every document.
        of: Vec<String>,
    },
    /// `<create type="Event, Session log" label="New event">` — a button that
    /// makes a new document of the type; of several, one chooses.
    Create {
        /// Names of the types, by id once stored.
        of: Vec<String>,
        /// What the button says — empty: `＋ New` and the type.
        #[serde(default)]
        label: String,
    },
    /// `<graph around=self> … </graph>` — people and their relations,
    /// drawn where it stands. Inside: where
    /// people were set by hand, and how single lines are drawn.
    Graph(Graph),
    /// `<map src=img:… height=480> … </map>` — a picture to move and zoom,
    /// with pins on it. A pin may lead to a document — one with a map of
    /// its own is a level to go to: a stair down to the cellar.
    Map(Map),
    /// `%%Name%%` — the value of a property of this document.
    ///
    /// Filled in when drawing: only there is it known what this reader has.
    Value { name: String },
    /// `<select bind=Species>` — an element that shows a property. Reading,
    /// it shows the value; the editor makes a field of it.
    Bound { element: String, property: String, label: String },
    /// A roll or a sum: `[[1d20 + Strength mod]]`, `[[+4]]` — or dice as
    /// they stand in text, `2d6 + 3`, `1W20`.
    ///
    /// Only cut out here; the display reckons it, since only there are the
    /// property values known.
    Roll {
        formula: String,
        /// The words as they stood in the text — empty for `[[…]]`, which
        /// shows what it reckons.
        #[serde(default)]
        text: String,
    },
    /// `<abilities show=Abilities with=mod short="STR, DEX, …">` — number
    /// properties of this document as the ability row of a stat block. `with`
    /// names a rule whose result each tile shows below its score — the
    /// modifier, as the world defines it; `short` heads the tiles.
    Abilities {
        show: Vec<String>,
        #[serde(default, skip_serializing_if = "String::is_empty")]
        with: String,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        short: Vec<String>,
        /// `also="Saving throws.Bonus"`: a second line on each tile — that
        /// number of the pick named like the score, rolled as a check.
        #[serde(default, skip_serializing_if = "String::is_empty")]
        also: String,
        /// `heading=Abilities`, `also-heading=Saves`: what stands over the
        /// scores, and over the line of what `also` shows.
        #[serde(default, skip_serializing_if = "String::is_empty")]
        heading: String,
        #[serde(default, skip_serializing_if = "String::is_empty")]
        also_heading: String,
    },
    /// `<embed of=Sessions>` — another document's text, drawn here under its
    /// name: a page made of pages. A type's page brings its create button.
    Embed {
        /// The document, by name — by id once stored.
        of: String,
        /// `fold=folded` starts it folded, `fold=off` lets it never fold.
        #[serde(default, skip_serializing_if = "String::is_empty")]
        fold: String,
        /// `title=off`: its text without the name above it.
        #[serde(default, skip_serializing_if = "String::is_empty")]
        title: String,
    },
    /// `<view of=Stats>` — the layout of a property group, drawn here with
    /// this document's values. Defined once in the group, it changes
    /// wherever it is shown. `of` holds the group's id once stored.
    View { of: String },
}

/// A map: its picture and the pins on it.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Map {
    /// The picture, an image of the world: `img:…`.
    pub src: String,
    /// How tall the map stands in the page, in pixels.
    pub height: u16,
    pub pins: Vec<MapPin>,
    /// Regions: shapes that light up under the pointer and lead somewhere.
    #[serde(default)]
    pub areas: Vec<MapArea>,
}

/// `<area at="10,20 30,20 30,40" label=Northland to=@@Region:Northland@@ color=green>`
/// — a region of the picture, its border point by point, in percent.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MapArea {
    /// The corners, in tenths of a percent — three at least.
    pub points: Vec<(i32, i32)>,
    pub label: String,
    pub to: String,
    pub color: String,
    /// Who sees it, where that is narrower than the map — see `pins` in
    /// [`crate::document`]. Only to mark it as such.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub view: String,
}

/// Where a pin's name stands beside it: `label-at=top`, and with `hover`
/// only under the pointer — `label-at="top hover"`.
pub const LABEL_AT: [&str; 4] = ["right", "left", "top", "bottom"];

/// The markers a pin may be, besides the pin itself.
pub const MARKERS: [&str; 20] = [
    "pin", "door", "ladder", "stairs", "chest", "skull", "castle", "tower", "house", "camp", "cave", "ship", "anchor", "tree", "mountain",
    "flag", "star", "sword", "eye", "question",
];

/// `<pin at=34.5,62 label="Stair down" to=@@Place:Cellar@@ color=amber>` —
/// a marker on the picture, where it lies in percent of its width and height.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MapPin {
    /// Tenths of a percent across, 0 to 1000.
    pub x: i32,
    /// Tenths of a percent down, 0 to 1000.
    pub y: i32,
    pub label: String,
    /// A document it leads to — by id once stored.
    pub to: String,
    /// One of [`COLOURS`] — empty as usual.
    pub color: String,
    /// One of [`MARKERS`] — empty: the pin.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub icon: String,
    /// One of [`LABEL_AT`] — empty: right of it.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub label_at: String,
    /// Its name only under the pointer.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub label_hover: bool,
    /// Who sees it, where that is narrower than the map.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub view: String,
}

/// How tall a map stands without saying.
pub const MAP_HEIGHT: u16 = 480;

/// Reads a map: its tag, and the pins between it and `</map>`.
fn read_map(tag: &str, inside: &[&str]) -> Map {
    let attrs = attributes(tag);
    let get = |n: &str| attrs.iter().find(|(a, _)| a == n).map(|(_, v)| v.clone()).unwrap_or_default();
    let mut m = Map {
        src: get("src").trim().to_string(),
        height: get("height").parse::<u16>().unwrap_or(MAP_HEIGHT).clamp(160, 2000),
        pins: Vec::new(),
        areas: Vec::new(),
    };
    // `34.5` percent as 345 tenths, kept within the picture.
    let tenths = |v: &str| v.trim().parse::<f64>().ok().filter(|n| n.is_finite()).map(|n| (n * 10.0).round().clamp(0.0, 1000.0) as i32);
    for line in inside {
        if let Some(raw) = line.trim().strip_prefix("<area").and_then(|l| l.strip_suffix('>')) {
            let attrs = attributes(raw.trim_end_matches('/'));
            let get = |n: &str| attrs.iter().find(|(a, _)| a == n).map(|(_, v)| v.clone()).unwrap_or_default();
            let points: Option<Vec<(i32, i32)>> = get("at")
                .split_whitespace()
                .map(|p| p.split_once(',').and_then(|(x, y)| Some((tenths(x)?, tenths(y)?))))
                .collect();
            let Some(points) = points.filter(|p| p.len() >= 3) else { continue };
            let color = get("color");
            m.areas.push(MapArea {
                points,
                label: get("label"),
                to: get("to").trim().to_string(),
                color: if COLOURS.contains(&color.as_str()) { color } else { String::new() },
                view: get("view").trim().to_string(),
            });
            continue;
        }
        let Some(raw) = line.trim().strip_prefix("<pin").and_then(|l| l.strip_suffix('>')) else { continue };
        let attrs = attributes(raw.trim_end_matches('/'));
        let get = |n: &str| attrs.iter().find(|(a, _)| a == n).map(|(_, v)| v.clone()).unwrap_or_default();
        let at = get("at");
        let Some((x, y)) = at.split_once(',').and_then(|(x, y)| Some((tenths(x)?, tenths(y)?))) else { continue };
        let color = get("color");
        m.pins.push(MapPin {
            x,
            y,
            label: get("label"),
            to: get("to").trim().to_string(),
            color: if COLOURS.contains(&color.as_str()) { color } else { String::new() },
            icon: Some(get("icon")).filter(|i| MARKERS.contains(&i.as_str()) && i != "pin").unwrap_or_default(),
            label_at: get("label-at").split_whitespace().find(|w| LABEL_AT.contains(w) && *w != "right").unwrap_or_default().to_string(),
            label_hover: get("label-at").split_whitespace().any(|w| w == "hover"),
            view: get("view").trim().to_string(),
        });
    }
    m
}

/// Whether a line opens a map: `<map …>` on a line of its own.
fn opens_map(line: &str) -> Option<&str> {
    let t = line.trim();
    let inner = t.strip_prefix("<map")?.strip_suffix('>')?;
    (inner.is_empty() || inner.starts_with(char::is_whitespace)).then_some(inner)
}

/// A relation graph: who is drawn, and how.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Graph {
    /// Only people of this type and its subtypes — empty for those the
    /// relations name.
    pub of: String,
    /// The relation properties drawn — empty for all of them.
    pub via: Vec<String>,
    /// Only who is related to this document within `depth` steps; `self`
    /// is the document the text belongs to.
    pub around: String,
    pub depth: u8,
    /// Where people were set by hand. Whoever stands here is drawn even
    /// without a relation.
    pub people: Vec<GraphPerson>,
    /// Lines drawn other than by default.
    pub lines: Vec<GraphLine>,
    /// What every card and line shows: `portrait`, `type`, `roles` — empty
    /// for all of them.
    pub show: Vec<String>,
}

/// `<person who=Tim at=12,6 color=amber sub=note note="The heir">` —
/// someone set on the grid by hand, and how their card looks.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct GraphPerson {
    pub who: String,
    pub x: i32,
    pub y: i32,
    /// The card's frame, one of [`COLOURS`] — empty as usual.
    pub color: String,
    /// What stands under the name: `type`, `role`, `note`, `none` — empty
    /// for the type.
    pub sub: String,
    /// The text under the name when `sub=note`.
    pub note: String,
}

/// `<line between="Tim, Simon" color=amber sides="bottom, top" shape=curve label=30>`
/// — how the line between two people is drawn. What it does not say is
/// chosen as usual.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct GraphLine {
    /// Its two ends: someone — `Tim` — or the line between two — `Anna &
    /// Franklin`. A line between lines relates everyone at one end to
    /// everyone at the other: two parents and their two sons.
    pub between: Vec<String>,
    /// One of [`COLOURS`] — empty: by the pair of roles.
    pub color: String,
    /// Where it leaves each card, in the order of `between`: `top`,
    /// `right`, `bottom`, `left`, with how far along that side in percent —
    /// `bottom 30` — or empty for the best.
    pub sides: Vec<String>,
    /// Arrowheads: `end` (at the second), `start`, `both` — empty for none.
    pub arrow: String,
    /// For an end that is a line: how far along it, 0 to 100, in the order
    /// of `between` — empty for its middle.
    pub dock: Vec<String>,
    /// For an end of several, in the order of `between`: `each` — each
    /// one's role — or empty: the roles once.
    pub group: Vec<String>,
    /// For an end of several: a word in place of their roles — `Parents`.
    pub say: Vec<String>,
    /// `curve`, `straight` or `corner` — empty: a curve.
    pub shape: String,
    /// Where the roles stand along the line, 0 to 100, or `none` — empty:
    /// beside whom they describe.
    pub label: String,
}

/// The colours a line may have — the same names everywhere.
pub const COLOURS: [&str; 9] = ["teal", "amber", "blue", "rose", "green", "violet", "red", "cyan", "grey"];
const SIDES: [&str; 4] = ["top", "right", "bottom", "left"];
const SHAPES: [&str; 3] = ["curve", "straight", "corner"];
const ARROWS: [&str; 3] = ["end", "start", "both"];
const SHOWN: [&str; 3] = ["portrait", "type", "roles"];
const SUBS: [&str; 4] = ["type", "role", "note", "none"];

/// `bottom` or `bottom 30` — a side, and how far along it.
fn side(text: &str) -> String {
    let mut words = text.split_whitespace();
    let (Some(side), along) = (words.next(), words.next()) else { return String::new() };
    if !SIDES.contains(&side) || words.next().is_some() {
        return String::new();
    }
    match along.map(|a| a.parse::<u8>()) {
        None => side.to_string(),
        Some(Ok(n)) if n <= 100 => if n == 50 { side.to_string() } else { format!("{side} {n}") },
        Some(_) => side.to_string(),
    }
}

/// Reads a graph: its tag, and the lines between it and `</graph>`.
///
/// Forgiving like a list: what does not read is left out. A graph that
/// forgets a line's colour is better than a paragraph that vanishes.
fn read_graph(tag: &str, inside: &[&str]) -> Graph {
    let mut g = Graph { depth: 1, ..Graph::default() };
    for (attr, value) in attributes(tag) {
        match attr.as_str() {
            "type" => g.of = value,
            "via" => g.via = listed(&value),
            "around" => g.around = value,
            "depth" => g.depth = value.parse::<u8>().unwrap_or(1).clamp(1, 5),
            "show" => g.show = listed(&value).into_iter().filter(|s| SHOWN.contains(&s.as_str())).collect(),
            _ => {}
        }
    }
    for line in inside {
        let Some(raw) = line.trim().strip_prefix('<').and_then(|l| l.strip_suffix('>')) else { continue };
        let raw = raw.trim_end_matches('/');
        let (name, rest) = raw.split_once(char::is_whitespace).unwrap_or((raw, ""));
        let attrs = attributes(rest);
        let get = |n: &str| attrs.iter().find(|(a, _)| a == n).map(|(_, v)| v.clone()).unwrap_or_default();
        match name.to_ascii_lowercase().as_str() {
            "person" => {
                let at = get("at");
                let place = at.split_once(',').and_then(|(x, y)| Some((x.trim().parse().ok()?, y.trim().parse().ok()?)));
                let who = get("who");
                let color = get("color");
                let sub = get("sub");
                if let (false, Some((x, y))) = (who.trim().is_empty(), place) {
                    g.people.push(GraphPerson {
                        who: who.trim().to_string(),
                        x,
                        y,
                        color: if COLOURS.contains(&color.as_str()) { color } else { String::new() },
                        sub: if SUBS.contains(&sub.as_str()) { sub } else { String::new() },
                        note: get("note"),
                    });
                }
            }
            "line" => {
                // Each end someone, or two joined by `&`.
                let between: Vec<String> = get("between")
                    .split(',')
                    .map(|end| end.split('&').map(str::trim).filter(|w| !w.is_empty()).collect::<Vec<_>>().join(" & "))
                    .collect();
                let fits = |end: &String| (1..=2).contains(&end.split(" & ").filter(|w| !w.is_empty()).count());
                if between.len() != 2 || !between.iter().all(fits) {
                    continue;
                }
                let dock: Vec<String> = get("dock")
                    .split(',')
                    .map(|d| match d.trim().parse::<u8>() {
                        Ok(n) if n <= 100 && n != 50 => n.to_string(),
                        _ => String::new(),
                    })
                    .collect();
                let group: Vec<String> =
                    get("group").split(',').map(|v| if v.trim() == "each" { "each".to_string() } else { String::new() }).collect();
                let say: Vec<String> = get("say").split(',').map(|v| v.trim().to_string()).collect();
                let color = get("color");
                let shape = get("shape");
                let label = get("label");
                let sides: Vec<String> = get("sides").split(',').map(side).collect();
                let arrow = get("arrow");
                g.lines.push(GraphLine {
                    between,
                    color: if COLOURS.contains(&color.as_str()) { color } else { String::new() },
                    sides: if sides.iter().all(String::is_empty) { Vec::new() } else { sides },
                    shape: if SHAPES.contains(&shape.as_str()) { shape } else { String::new() },
                    arrow: if ARROWS.contains(&arrow.as_str()) { arrow } else { String::new() },
                    dock: if dock.iter().all(String::is_empty) { Vec::new() } else { dock },
                    group: if group.iter().all(String::is_empty) { Vec::new() } else { group },
                    say: if say.iter().all(String::is_empty) { Vec::new() } else { say },
                    label: if label == "none" || label.parse::<u8>().is_ok_and(|n| n <= 100) { label } else { String::new() },
                });
            }
            _ => {}
        }
    }
    g
}

/// Whether a line opens a graph: `<graph …>` on a line of its own.
fn opens_graph(line: &str) -> Option<&str> {
    let t = line.trim();
    let inner = t.strip_prefix("<graph")?.strip_suffix('>')?;
    (inner.is_empty() || inner.starts_with(char::is_whitespace)).then_some(inner)
}

/// An image or an ability row on a line of its own: a block, as the editor
/// shows it.
fn image_line(line: &str) -> Option<Node> {
    let t = line.trim();
    if !t.starts_with("<image") && !t.starts_with("<abilities") && !t.starts_with("<view") {
        return None;
    }
    read_tag(t).filter(|n| matches!(n, Node::Image { .. } | Node::Abilities { .. } | Node::View { .. }))
}

/// Splits Markdown into a tree.
///
/// A graph is cut out first: it stands on lines of its own, from `<graph …>`
/// to `</graph>`, and what is inside is no Markdown. A graph without its
/// closing tag is the line alone. An image on a line of its own is cut out
/// the same way — what follows it is a paragraph again, not part of an HTML
/// block.
pub fn split(markdown: &str) -> Vec<Node> {
    let lines: Vec<&str> = markdown.split_inclusive('\n').collect();
    let mut out = Vec::new();
    let mut text = String::new();
    let mut i = 0;
    while i < lines.len() {
        if let Some(image) = image_line(lines[i]) {
            out.extend(split_markdown(&std::mem::take(&mut text)));
            out.push(image);
            i += 1;
            continue;
        }
        if let Some(tag) = opens_map(lines[i]) {
            out.extend(split_markdown(&std::mem::take(&mut text)));
            let close = lines[i + 1..].iter().position(|l| l.trim().eq_ignore_ascii_case("</map>"));
            let inside = close.map(|c| &lines[i + 1..i + 1 + c]).unwrap_or(&[]);
            out.push(Node::Map(read_map(tag, inside)));
            i += 1 + close.map(|c| c + 1).unwrap_or(0);
            continue;
        }
        let Some(tag) = opens_graph(lines[i]) else {
            text.push_str(lines[i]);
            i += 1;
            continue;
        };
        out.extend(split_markdown(&std::mem::take(&mut text)));
        let close = lines[i + 1..].iter().position(|l| l.trim().eq_ignore_ascii_case("</graph>"));
        let inside = close.map(|c| &lines[i + 1..i + 1 + c]).unwrap_or(&[]);
        out.push(Node::Graph(read_graph(tag, inside)));
        i += 1 + close.map(|c| c + 1).unwrap_or(0);
    }
    out.extend(split_markdown(&text));
    out
}

fn split_markdown(markdown: &str) -> Vec<Node> {
    if markdown.trim().is_empty() {
        return Vec::new();
    }
    let mut options = Options::empty();
    options.insert(Options::ENABLE_STRIKETHROUGH);
    options.insert(Options::ENABLE_TABLES);
    let raw: Vec<Event> = Parser::new_ext(markdown, options).collect();

    let mut stack: Vec<(Frame, Vec<Node>)> = vec![(Frame::Root, Vec::new())];
    let mut list_items: Vec<Vec<Vec<Node>>> = Vec::new();
    // Tables being read: alignment, head cells, rows, the row being read.
    let mut tables: Vec<(Vec<String>, Vec<Vec<Node>>, Vec<Vec<Vec<Node>>>, Vec<Vec<Node>>)> = Vec::new();

    for event in raw {
        match event {
            Event::Start(tag) => match tag {
                Tag::Paragraph => stack.push((Frame::Paragraph, Vec::new())),
                Tag::Heading { level, .. } => {
                    stack.push((Frame::Heading(level_from(level)), Vec::new()))
                }
                Tag::BlockQuote(_) => stack.push((Frame::Quote, Vec::new())),
                Tag::List(start) => {
                    list_items.push(Vec::new());
                    stack.push((Frame::List(start.is_some()), Vec::new()));
                }
                Tag::Item => stack.push((Frame::Item, Vec::new())),
                Tag::Table(align) => {
                    let align = align
                        .iter()
                        .map(|a| match a {
                            pulldown_cmark::Alignment::Left => "left",
                            pulldown_cmark::Alignment::Center => "center",
                            pulldown_cmark::Alignment::Right => "right",
                            pulldown_cmark::Alignment::None => "",
                        }.to_string())
                        .collect();
                    tables.push((align, Vec::new(), Vec::new(), Vec::new()));
                    stack.push((Frame::Table, Vec::new()));
                }
                Tag::TableHead => stack.push((Frame::TableHead, Vec::new())),
                Tag::TableRow => stack.push((Frame::TableRow, Vec::new())),
                Tag::TableCell => stack.push((Frame::TableCell, Vec::new())),
                Tag::Emphasis => stack.push((Frame::Emphasis, Vec::new())),
                Tag::Strong => stack.push((Frame::Strong, Vec::new())),
                Tag::Strikethrough => stack.push((Frame::Strikethrough, Vec::new())),
                Tag::Link { dest_url, .. } => {
                    stack.push((Frame::Link(dest_url.to_string()), Vec::new()))
                }
                Tag::CodeBlock(kind) => {
                    let language = match kind {
                        pulldown_cmark::CodeBlockKind::Fenced(s) => s.to_string(),
                        pulldown_cmark::CodeBlockKind::Indented => String::new(),
                    };
                    stack.push((Frame::CodeBlock(language), Vec::new()));
                }
                // Everything else — tables, images, footnotes — is passed
                // through as content but not interpreted as structure.
                _ => stack.push((Frame::PassThrough, Vec::new())),
            },
            Event::End(end) => {
                let Some((frame, children)) = stack.pop() else {
                    continue;
                };
                match (frame, end) {
                    (Frame::TableCell, _) => {
                        if let Some(table) = tables.last_mut() {
                            table.3.push(tidy(children));
                        }
                    }
                    (Frame::TableHead, _) => {
                        if let Some(table) = tables.last_mut() {
                            table.1 = std::mem::take(&mut table.3);
                        }
                    }
                    (Frame::TableRow, _) => {
                        if let Some(table) = tables.last_mut() {
                            let row = std::mem::take(&mut table.3);
                            table.2.push(row);
                        }
                    }
                    (Frame::Table, _) => {
                        if let Some((align, head, rows, _)) = tables.pop() {
                            append(&mut stack, Node::Table { align, head, rows });
                        }
                    }
                    (Frame::Item, _) => {
                        if let Some(items) = list_items.last_mut() {
                            items.push(tidy(children));
                        }
                    }
                    // What is passed through loses its frame but keeps all of
                    // its content — an HTML block may hold several tags.
                    (Frame::PassThrough, _) => {
                        for node in tidy(children) {
                            append(&mut stack, node);
                        }
                    }
                    (Frame::List(ordered), _) => {
                        let items = list_items.pop().unwrap_or_default();
                        append(&mut stack, Node::List { ordered, items });
                    }
                    (Frame::CodeBlock(language), _) => {
                        // Code stays code: nothing inside it gets linked.
                        append(
                            &mut stack,
                            Node::CodeBlock {
                                language,
                                text: children
                                    .iter()
                                    .map(|k| match k {
                                        Node::Text { text } => text.as_str(),
                                        _ => "",
                                    })
                                    .collect(),
                            },
                        );
                    }
                    (frame, _) => {
                        if let Some(nodes) = frame.close(tidy(children)) {
                            append(&mut stack, nodes);
                        }
                    }
                }
            }
            // Deliberately appended unsplit: a link may arrive in several
            // events. Links are only cut out when the frame closes.
            Event::Text(text) => append(
                &mut stack,
                Node::Text {
                    text: text.to_string(),
                },
            ),
            Event::Code(text) => append(
                &mut stack,
                Node::Code {
                    text: text.to_string(),
                },
            ),
            Event::SoftBreak | Event::HardBreak => append(&mut stack, Node::SoftBreak),
            Event::Rule => append(&mut stack, Node::Rule),
            // Of all tags exactly two are interpreted: `<image>` and `<list>`.
            // Everything else — and with it all raw HTML — stays text. That
            // is exactly the point: it is shown, not executed.
            //
            // An HTML block runs up to the next blank line, so several tags
            // can arrive in one event — each line is read on its own.
            Event::Html(text) | Event::InlineHtml(text) => {
                for line in text.split_inclusive('\n') {
                    let bare = line.trim();
                    if bare.is_empty() || bare.eq_ignore_ascii_case("</list>") {
                        continue;
                    }
                    let nodes = read_tag(bare).unwrap_or(Node::Text {
                        text: line.to_string(),
                    });
                    append(&mut stack, nodes);
                }
            }
            _ => {}
        }
    }

    stack
        .pop()
        .map(|(_, children)| tidy(children))
        .unwrap_or_default()
}

/// Merges adjacent pieces of text and then splits out the links.
///
/// The order matters: the parser may hand over a piece of text in several
/// events. Only once put together can a link in it be recognised.
fn tidy(children: Vec<Node>) -> Vec<Node> {
    let mut merged: Vec<Node> = Vec::with_capacity(children.len());
    for nodes in children {
        match (&nodes, merged.last_mut()) {
            (Node::Text { text }, Some(Node::Text { text: before })) => before.push_str(text),
            _ => merged.push(nodes),
        }
    }

    let mut out = Vec::with_capacity(merged.len());
    let ends: Vec<bool> = (0..merged.len()).map(|i| matches!(merged.get(i + 1), None | Some(Node::SoftBreak))).collect();
    for (nodes, line_end) in merged.into_iter().zip(ends) {
        match nodes {
            Node::Text { text } => match trailing_mark(&text).filter(|_| line_end) {
                Some((before, id)) => {
                    out.extend(wikilinks(before));
                    out.push(Node::Anchor { id: id.to_string() });
                }
                None => out.extend(wikilinks(&text)),
            },
            other => out.push(other),
        }
    }
    out
}

/// `… planen. ^2f9e17` — the text before the mark, and the mark.
fn trailing_mark(text: &str) -> Option<(&str, &str)> {
    let bare = text.trim_end();
    let at = bare.rfind(|c: char| c.is_whitespace()).map_or(0, |i| i + 1);
    let id = bare[at..].strip_prefix('^')?;
    if id.is_empty() || id.chars().count() > 64 || !id.chars().all(|c| c.is_alphanumeric() || c == '-') {
        return None;
    }
    Some((bare[..at].trim_end(), id))
}

#[derive(Clone, Debug)]
enum Frame {
    Root,
    Paragraph,
    Heading(u8),
    Quote,
    List(bool),
    Item,
    CodeBlock(String),
    Strong,
    Emphasis,
    Strikethrough,
    Link(String),
    Table,
    TableHead,
    TableRow,
    TableCell,
    PassThrough,
}

impl Frame {
    fn close(self, children: Vec<Node>) -> Option<Node> {
        Some(match self {
            Frame::Paragraph => Node::Paragraph { children },
            Frame::Heading(level) => Node::Heading { level, children },
            Frame::Quote => Node::Quote { children },
            Frame::Strong => Node::Strong { children },
            Frame::Emphasis => Node::Emphasis { children },
            Frame::Strikethrough => Node::Strikethrough { children },
            Frame::Link(target) => Node::Link { target, children },
            Frame::Root
            | Frame::List(_)
            | Frame::Item
            | Frame::CodeBlock(_)
            | Frame::Table
            | Frame::TableHead
            | Frame::TableRow
            | Frame::TableCell
            | Frame::PassThrough => {
                return None
            }
        })
    }
}

fn append(stack: &mut [(Frame, Vec<Node>)], nodes: Node) {
    if let Some((_, children)) = stack.last_mut() {
        children.push(nodes);
    }
}

fn level_from(level: HeadingLevel) -> u8 {
    match level {
        HeadingLevel::H1 => 1,
        HeadingLevel::H2 => 2,
        HeadingLevel::H3 => 3,
        HeadingLevel::H4 => 4,
        HeadingLevel::H5 => 5,
        HeadingLevel::H6 => 6,
    }
}

/// Interprets `<image …>`, `<list …>`, `<timeline …>`, `<calendar …>`,
/// `<embed …>` and the other elements below.
///
/// Deliberately only these tags and deliberately here, not in the frontend:
/// what does not come through this function can never become an element
/// later. A tag with another name returns `None` and so stays text.
fn read_tag(raw: &str) -> Option<Node> {
    let content = raw.trim().strip_prefix('<')?.trim_end();
    let content = content.strip_suffix('>')?.trim_end_matches('/').trim();
    let (name, rest) = match content.split_once(char::is_whitespace) {
        Some((n, r)) => (n, r),
        None => (content, ""),
    };
    if name.eq_ignore_ascii_case("list") {
        return Some(read_list(rest));
    }
    if name.eq_ignore_ascii_case("timeline") {
        let attrs = attributes(rest);
        let get = |n: &str| attrs.iter().find(|(a, _)| a == n).map(|(_, v)| v.clone()).unwrap_or_default();
        return Some(Node::Timeline {
            of: listed(&get("type")),
            show: listed(&get("show")),
            around: get("around"),
            until: get("until"),
            text: listed(&get("text")),
        });
    }
    if name.eq_ignore_ascii_case("abilities") {
        let attrs = attributes(rest);
        let get = |n: &str| attrs.iter().find(|(a, _)| a == n).map(|(_, v)| v.clone()).unwrap_or_default();
        return Some(Node::Abilities {
            show: listed(&get("show")),
            with: get("with").trim().to_string(),
            short: listed(&get("short")),
            also: get("also").trim().to_string(),
            heading: get("heading").trim().to_string(),
            also_heading: get("also-heading").trim().to_string(),
        });
    }
    if name.eq_ignore_ascii_case("view") {
        let attrs = attributes(rest);
        let of = attrs.iter().find(|(a, _)| a == "of").map(|(_, v)| v.trim().to_string()).unwrap_or_default();
        return Some(Node::View { of });
    }
    if name.eq_ignore_ascii_case("embed") {
        let attrs = attributes(rest);
        let get = |n: &str| attrs.iter().find(|(a, _)| a == n).map(|(_, v)| v.trim().to_string()).unwrap_or_default();
        return Some(Node::Embed { of: get("of"), fold: get("fold"), title: get("title") });
    }
    if name.eq_ignore_ascii_case("create") {
        let attrs = attributes(rest);
        let get = |n: &str| attrs.iter().find(|(a, _)| a == n).map(|(_, v)| v.clone()).unwrap_or_default();
        return Some(Node::Create { of: listed(&get("type")), label: get("label").trim().to_string() });
    }
    if name.eq_ignore_ascii_case("calendar") {
        let attrs = attributes(rest);
        let of = attrs.iter().find(|(a, _)| a == "type").map(|(_, v)| listed(v)).unwrap_or_default();
        return Some(Node::Calendar { of });
    }
    let attrs = attributes(rest);
    if let Some((_, property)) = attrs.iter().find(|(a, _)| a == "bind") {
        return Some(Node::Bound {
            element: name.to_lowercase(),
            property: property.clone(),
            label: attrs.iter().find(|(a, _)| a == "label").map(|(_, v)| v.clone()).unwrap_or_default(),
        });
    }
    if !name.eq_ignore_ascii_case("image") {
        return None;
    }

    let mut src = String::new();
    let mut stands_for = String::new();
    let mut align = String::from("center");
    let mut width = 0u16;
    let mut caption = String::new();
    let mut wrap = true;

    for (attr, value) in attributes(rest) {
        match attr.as_str() {
            "src" => src = value,
            "for" => stands_for = value,
            "align" if ["left", "center", "right"].contains(&value.as_str()) => {
                align = value
            }
            "width" => width = value.trim_end_matches("px").parse().unwrap_or(0),
            // `size=300x200` is the same notation as in the infobox.
            "size" => {
                if let Some((w, _)) = value.split_once(['x', 'X', '×']) {
                    width = w.trim().parse().unwrap_or(0);
                }
            }
            "caption" | "alt" => caption = value,
            "wrap" => wrap = !matches!(value.as_str(), "no" | "false" | "off"),
            _ => {}
        }
    }

    Some(Node::Image {
        src,
        wrap: wrap && align != "center",
        align,
        width,
        caption,
        stands_for,
    })
}

/// Reads `<list type=Place style=cards show="image, Climate" where="Role=NPC" below=Europe>`.
///
/// Forgiving on purpose: an unknown word is ignored and a column count out of
/// range is clamped. A list that shows a little too much is better than a
/// paragraph that vanishes while someone types.
/// How a `<list>` is drawn: `style=list|cards|tree|table`.
/// `inline`: in the line, by commas — `**Skills** <list … style=inline>` in a short stat block.
const STYLES: [&str; 5] = ["list", "cards", "tree", "table", "inline"];

/// A comma-separated attribute value, trimmed.
fn listed(value: &str) -> Vec<String> {
    value.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect()
}

fn read_list(rest: &str) -> Node {
    let mut of = String::new();
    let mut style = String::from("list");
    let mut columns = 1u8;
    let mut show = Vec::new();
    let mut filter = Vec::new();
    let mut below = String::new();
    let mut from = String::new();
    let mut when = String::new();
    let mut choose = String::new();
    let mut sort = Vec::new();
    let mut count = String::new();
    let mut by = String::new();
    let mut add = String::new();

    for (attr, value) in attributes(rest) {
        match attr.as_str() {
            "type" => of = value,
            "style" if STYLES.contains(&value.as_str()) => style = value,
            "columns" => columns = value.parse::<u8>().unwrap_or(1).clamp(1, 3),
            "show" => show = shown_list(&value),
            "where" => {
                for condition in value.split(',') {
                    if let Some((field, wanted)) = condition.split_once('=') {
                        filter.push((field.trim().to_string(), wanted.trim().to_string()));
                    }
                }
            }
            "below" => below = value,
            "from" => from = value,
            "when" => when = value,
            "filter" => choose = value.trim().to_string(),
            "sort" => sort = listed(&value),
            "count" => count = value.trim().to_string(),
            "by" => by = value.trim().to_string(),
            "add" => add = value.trim().to_string(),
            _ => {}
        }
    }

    Node::Listing {
        of,
        style,
        columns,
        show,
        filter,
        below,
        from,
        when,
        choose,
        sort,
        count,
        by,
        add,
    }
}

/// What a list shows, by comma — a roll in `[[…]]` stays whole, commas and all:
/// `Level, [[if(Level > 2, 1d8, 1d6)]]`.
pub(crate) fn shown_list(value: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut depth = 0usize;
    let mut part = String::new();
    let chars: Vec<char> = value.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '[' && chars.get(i + 1) == Some(&'[') {
            depth += 1;
            part.push_str("[[");
            i += 2;
            continue;
        }
        if chars[i] == ']' && chars.get(i + 1) == Some(&']') && depth > 0 {
            depth -= 1;
            part.push_str("]]");
            i += 2;
            continue;
        }
        if chars[i] == ',' && depth == 0 {
            out.push(part.trim().to_string());
            part.clear();
        } else {
            part.push(chars[i]);
        }
        i += 1;
    }
    out.push(part.trim().to_string());
    out.retain(|s| !s.is_empty());
    out
}

/// Reads `name=value` and `name="value with spaces"`.
fn attributes(rest: &str) -> Vec<(String, String)> {
    let chars: Vec<char> = rest.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;

    while i < chars.len() {
        while i < chars.len() && chars[i].is_whitespace() {
            i += 1;
        }
        let begin = i;
        while i < chars.len() && !chars[i].is_whitespace() && chars[i] != '=' {
            i += 1;
        }
        if i == begin {
            break;
        }
        let name: String = chars[begin..i].iter().collect::<String>().to_lowercase();
        if i < chars.len() && chars[i] == '=' {
            i += 1;
            let value = if i < chars.len() && chars[i] == '"' {
                i += 1;
                let start = i;
                while i < chars.len() && chars[i] != '"' {
                    i += 1;
                }
                let w: String = chars[start..i].iter().collect();
                i += 1;
                w
            } else {
                let start = i;
                while i < chars.len() && !chars[i].is_whitespace() {
                    i += 1;
                }
                chars[start..i].iter().collect()
            };
            out.push((name, value));
        } else {
            out.push((name, String::new()));
        }
    }
    out
}

/// Splits the links out of a piece of text — see [`crate::link`].
///
/// Markdown does not know this notation, so it arrives as ordinary text and
/// is cut out here.
fn wikilinks(text: &str) -> Vec<Node> {
    crate::link::scan(text)
        .into_iter()
        .flat_map(|piece| match piece {
            crate::link::Piece::Text(text) => values(text).into_iter().flat_map(rolls).collect(),
            crate::link::Piece::Link(link) => vec![Node::Wikilink {
                target: link.name,
                text: link.text,
                of: link.of,
                property: link.property,
            }],
        })
        .collect()
}

/// Splits `%%Name%%` out of a piece of text.
fn values(text: &str) -> Vec<Node> {
    let mut out = Vec::new();
    let mut plain = 0;
    let mut from = 0;
    while let Some(open) = text[from..].find("%%").map(|i| i + from) {
        let Some(close) = text[open + 2..].find("%%").map(|i| i + open + 2) else { break };
        let name = text[open + 2..close].trim();
        if name.is_empty() || name.contains('\n') {
            from = close;
            continue;
        }
        if plain < open {
            out.push(Node::Text { text: text[plain..open].to_string() });
        }
        out.push(Node::Value { name: name.to_string() });
        plain = close + 2;
        from = plain;
    }
    if plain < text.len() {
        out.push(Node::Text { text: text[plain..].to_string() });
    }
    out
}

/// Cuts rolls out of a piece of plain text: `[[…]]`, and dice as written —
/// `2d6 + 3`, `1W20`, `d8`. German writes `W` for the die.
fn rolls(node: Node) -> Vec<Node> {
    let Node::Text { text } = node else { return vec![node] };
    let chars: Vec<(usize, char)> = text.char_indices().collect();
    let mut out = Vec::new();
    let mut plain = 0;
    let mut i = 0;
    while i < chars.len() {
        let (at, c) = chars[i];
        if c == '[' && text[at..].starts_with("[[") {
            if let Some(len) = text[at + 2..].find("]]") {
                let formula = text[at + 2..at + 2 + len].trim();
                if !formula.is_empty() && !formula.contains('\n') {
                    if plain < at {
                        out.push(Node::Text { text: text[plain..at].to_string() });
                    }
                    out.push(Node::Roll { formula: formula.to_string(), text: String::new() });
                    plain = at + 2 + len + 2;
                    while i < chars.len() && chars[i].0 < plain {
                        i += 1;
                    }
                    continue;
                }
            }
        }
        let after_word = i > 0 && chars[i - 1].1.is_alphanumeric();
        if !after_word {
            if let Some(end) = dice_at(&text, at) {
                if plain < at {
                    out.push(Node::Text { text: text[plain..at].to_string() });
                }
                let written = &text[at..end];
                out.push(Node::Roll { formula: written.to_string(), text: written.to_string() });
                plain = end;
                while i < chars.len() && chars[i].0 < plain {
                    i += 1;
                }
                continue;
            }
        }
        i += 1;
    }
    if plain == 0 {
        return vec![Node::Text { text }];
    }
    if plain < text.len() {
        out.push(Node::Text { text: text[plain..].to_string() });
    }
    out
}

/// One die term at `at` — `2d6`, `d20`, `1W8` — and what follows of the
/// sum: ` + 3`, `-1`, `+ 1d4`. Where it ends, or `None`. A word that merely
/// looks like one — `d6x`, `W3C` — is no roll.
fn dice_at(text: &str, at: usize) -> Option<usize> {
    let bytes = text.as_bytes();
    let digits = |from: usize, max: usize| {
        let mut j = from;
        while j < bytes.len() && j - from < max && bytes[j].is_ascii_digit() {
            j += 1;
        }
        j
    };
    // A die: count, `d` or `W`, sides — the end, or none.
    let die = |from: usize| -> Option<usize> {
        let count = digits(from, 3);
        let letter = *bytes.get(count)?;
        if !matches!(letter, b'd' | b'D' | b'w' | b'W') {
            return None;
        }
        let sides = digits(count + 1, 4);
        if sides <= count + 1 {
            return None;
        }
        // `2W20kh1`, `4d6kl3`: the highest or lowest that count.
        if matches!(bytes.get(sides), Some(b'k' | b'K')) && matches!(bytes.get(sides + 1), Some(b'h' | b'l' | b'H' | b'L')) {
            let kept = digits(sides + 2, 3);
            if kept > sides + 2 {
                return Some(kept);
            }
        }
        Some(sides)
    };
    let ends_word = |j: usize| text[j..].chars().next().is_none_or(|c| !c.is_alphanumeric());
    let mut end = die(at)?;
    if !ends_word(end) {
        return None;
    }
    // More of the sum: a sign, then a die or a number.
    loop {
        let mut j = end;
        while j < bytes.len() && bytes[j] == b' ' {
            j += 1;
        }
        if !matches!(bytes.get(j), Some(b'+' | b'-')) {
            break;
        }
        j += 1;
        while j < bytes.len() && bytes[j] == b' ' {
            j += 1;
        }
        let next = die(j).or_else(|| Some(digits(j, 4)).filter(|&k| k > j));
        match next {
            Some(k) if ends_word(k) => end = k,
            _ => break,
        }
    }
    Some(end)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(t: &str) -> Node {
        Node::Text { text: t.into() }
    }

    #[test]
    fn a_paragraph_stays_a_paragraph() {
        assert_eq!(
            split("Hello world"),
            vec![Node::Paragraph {
                children: vec![text("Hello world")]
            }]
        );
    }

    #[test]
    fn a_table_has_its_head_rows_and_links() {
        let tree = split("| Level | Feature |\n|:-|--:|\n| 1st | @@Feat:Alert@@ |\n| 2nd | **Rage** |");
        let Node::Table { align, head, rows } = &tree[0] else { panic!("{tree:?}") };
        assert_eq!(align, &["left", "right"]);
        assert_eq!(head[0], vec![text("Level")]);
        assert_eq!(rows.len(), 2);
        assert!(matches!(&rows[0][1][0], Node::Wikilink { target, .. } if target == "Alert"), "{:?}", rows[0][1]);
        assert!(matches!(&rows[1][1][0], Node::Strong { .. }));
    }

    #[test]
    fn a_mark_closing_a_line_is_a_place_to_lead_to() {
        let mark = |id: &str| Node::Anchor { id: id.into() };
        assert_eq!(split("They plan.   ^2f9e17"), vec![Node::Paragraph { children: vec![text("They plan."), mark("2f9e17")] }]);
        // After a link, at a line's end in the middle of a paragraph, in a table.
        let tree = split("He is @@Tim@@. ^9c2b7a\nNext line.");
        let Node::Paragraph { children } = &tree[0] else { panic!("{tree:?}") };
        assert_eq!(children[2..4], [text("."), mark("9c2b7a")]);
        assert!(matches!(children[4], Node::SoftBreak));
        let tree = split("| a | b |\n|-|-|\n| Very rare | 5,000 gold ^dbbde3 |");
        let Node::Table { rows, .. } = &tree[0] else { panic!("{tree:?}") };
        assert_eq!(rows[0][1], vec![text("5,000 gold"), mark("dbbde3")]);
        // Mid-line, or not a plain name: text.
        assert_eq!(split("x^2 is ^big here"), vec![Node::Paragraph { children: vec![text("x^2 is ^big here")] }]);
        assert_eq!(split("2^10"), vec![Node::Paragraph { children: vec![text("2^10")] }]);
        // A name of letters, any letters.
        assert_eq!(split("Preise. ^magische-belohnungen"), vec![Node::Paragraph { children: vec![text("Preise."), mark("magische-belohnungen")] }]);
    }

    #[test]
    fn headings_keep_their_level() {
        let tree = split("# Big\n\n### Small");
        assert_eq!(
            tree,
            vec![
                Node::Heading {
                    level: 1,
                    children: vec![text("Big")]
                },
                Node::Heading {
                    level: 3,
                    children: vec![text("Small")]
                },
            ]
        );
    }

    #[test]
    fn emphasis_nests() {
        let tree = split("A **bold *and slanted* text**");
        let Node::Paragraph { children } = &tree[0] else {
            panic!("not a paragraph: {tree:?}");
        };
        assert_eq!(children[0], text("A "));
        let Node::Strong { children: inner } = &children[1] else {
            panic!("not bold: {children:?}");
        };
        assert_eq!(inner[0], text("bold "));
        assert!(matches!(inner[1], Node::Emphasis { .. }));
    }

    #[test]
    fn lists_keep_their_items() {
        let tree = split("- one\n- two");
        let Node::List { ordered, items } = &tree[0] else {
            panic!("not a list: {tree:?}");
        };
        assert!(!ordered);
        assert_eq!(items.len(), 2);
        assert_eq!(items[0][0], text("one"));
    }

    #[test]
    fn ordered_lists_are_recognisable() {
        let tree = split("1. one\n2. two");
        let Node::List { ordered, .. } = &tree[0] else {
            panic!("not a list");
        };
        assert!(ordered);
    }

    #[test]
    fn links_are_split_out() {
        let tree = split("He lives in @@Place:Morningstone@@ by the river.");
        let Node::Paragraph { children } = &tree[0] else {
            panic!("not a paragraph");
        };
        assert_eq!(children[0], text("He lives in "));
        assert_eq!(
            children[1],
            Node::Wikilink {
                target: "Morningstone".into(),
                text: "Morningstone".into(),
                of: "Place".into(),
                property: String::new(),
            }
        );
        assert_eq!(children[2], text(" by the river."));
    }

    #[test]
    fn a_link_may_carry_its_own_text() {
        let tree = split("He is an @@Species:Elves|Elf@@ and speaks @@Language:High Elven@@.");
        let Node::Paragraph { children } = &tree[0] else {
            panic!("not a paragraph");
        };
        assert_eq!(
            children[1],
            Node::Wikilink { target: "Elves".into(), text: "Elf".into(), of: "Species".into(), property: String::new() }
        );
        assert_eq!(
            children[3],
            Node::Wikilink { target: "High Elven".into(), text: "High Elven".into(), of: "Language".into(), property: String::new() }
        );
    }

    #[test]
    fn values_and_bound_elements_are_nodes() {
        let tree = split("She is %%Age%% years old.\n\n<select bind=Species label=Kind>");
        let Node::Paragraph { children } = &tree[0] else { panic!("not a paragraph") };
        assert_eq!(children[1], Node::Value { name: "Age".into() });
        let flat = format!("{tree:?}");
        assert!(flat.contains("Bound { element: \"select\", property: \"Species\", label: \"Kind\" }"), "{flat}");
    }

    #[test]
    fn an_open_link_stays_text() {
        // No closing `@@`: that is not a link but text.
        let tree = split("A @@half link");
        let Node::Paragraph { children } = &tree[0] else {
            panic!("not a paragraph");
        };
        assert_eq!(children, &[text("A @@half link")]);
    }

    #[test]
    fn wrap_no_keeps_the_text_off_the_side() {
        let tree = split("<image src=img:a align=right wrap=no>\n<image src=img:b align=left>\n<image src=img:c align=center>");
        let wraps: Vec<bool> = tree
            .iter()
            .filter_map(|n| match n {
                Node::Image { wrap, .. } => Some(*wrap),
                _ => None,
            })
            .collect();
        assert_eq!(wraps, vec![false, true, false]);
    }

    #[test]
    fn text_right_after_an_image_line_is_a_paragraph() {
        let tree = split("<image src=img:a align=right>\nThe coast.\nA second line.\n<image src=img:b>\nAfter.");
        assert!(matches!(tree[0], Node::Image { .. }), "{tree:?}");
        assert!(matches!(tree[1], Node::Paragraph { .. }), "{tree:?}");
        assert!(matches!(tree[2], Node::Image { .. }), "{tree:?}");
        assert!(matches!(tree[3], Node::Paragraph { .. }), "{tree:?}");
        assert_eq!(tree.len(), 4, "{tree:?}");
    }

    #[test]
    fn an_image_in_a_paragraph_becomes_an_image_node() {
        let tree = split("Before.\n\n<image src=map align=right width=300 caption=\"The map\">\n\nAfter.");
        let flat = format!("{tree:?}");
        assert!(flat.contains("Image"), "no image node: {flat}");
        assert!(flat.contains("\"map\""));
        assert!(flat.contains("right"));
        assert!(flat.contains("300"));
        assert!(flat.contains("The map"));
    }

    #[test]
    fn a_foreign_tag_stays_text() {
        // Only `<image>` is interpreted. Everything else is text — otherwise
        // every new tag would be a new gap.
        for raw in ["<iframe src=x>", "<div onclick=evil>", "<img src=x>"] {
            let tree = split(raw);
            let flat = format!("{tree:?}");
            assert!(!flat.contains("Image"), "{raw} was interpreted: {flat}");
            assert!(flat.contains("Text"), "{raw} got lost: {flat}");
        }
    }

    #[test]
    fn raw_html_becomes_text_and_not_markup() {
        // The most important test in this file: whatever got through here
        // would later run in the application's window.
        let tree = split("<script>alert(1)</script>");
        let flat = format!("{tree:?}");
        assert!(flat.contains("Text"), "kept as text: {flat}");
        assert!(
            !flat.contains("Html"),
            "no node the frontend could interpret as markup"
        );
    }

    #[test]
    fn a_link_keeps_its_target() {
        let tree = split("[over there](https://example.org)");
        let Node::Paragraph { children } = &tree[0] else {
            panic!("not a paragraph");
        };
        assert_eq!(
            children[0],
            Node::Link {
                target: "https://example.org".into(),
                children: vec![text("over there")]
            }
        );
    }

    #[test]
    fn quote_and_rule_and_code() {
        let tree = split("> Quote\n\n---\n\n```rust\nfn a() {}\n```");
        assert!(matches!(tree[0], Node::Quote { .. }));
        assert_eq!(tree[1], Node::Rule);
        assert_eq!(
            tree[2],
            Node::CodeBlock {
                language: "rust".into(),
                text: "fn a() {}\n".into()
            }
        );
    }

    #[test]
    fn a_timeline_is_read() {
        let tree = split("Before.\n\n<timeline type=\"Event, Session\" show=Born around=self>\n\nAfter.");
        assert!(
            tree.iter().any(|n| matches!(n, Node::Timeline { of, show, around, .. }
                if of == &["Event", "Session"] && show == &["Born"] && around == "self")),
            "{tree:?}"
        );
    }

    #[test]
    fn a_timeline_says_which_text_it_shows() {
        let tree = split("<timeline type=Session text=\"Summary, Course\">");
        assert!(matches!(&tree[0], Node::Timeline { text, .. } if text == &["Summary", "Course"]), "{tree:?}");
    }

    #[test]
    fn a_calendar_is_read() {
        let tree = split("<calendar type=\"Session, Event\">");
        assert!(matches!(&tree[0], Node::Calendar { of } if of == &["Session", "Event"]), "{tree:?}");
    }

    #[test]
    fn a_list_can_show_what_a_property_picks() {
        let tree = split("<list from=Equipment style=table show=\"Rarity\">");
        assert!(matches!(&tree[0], Node::Listing { from, of, .. } if from == "Equipment" && of.is_empty()), "{tree:?}");
    }

    #[test]
    fn an_embed_names_the_document_it_shows() {
        assert_eq!(split("<embed of=\"Sessions\">"), vec![Node::Embed { of: "Sessions".into(), fold: String::new(), title: String::new() }]);
        assert_eq!(
            split("<embed of=Quests fold=folded title=off>"),
            vec![Node::Embed { of: "Quests".into(), fold: "folded".into(), title: "off".into() }]
        );
    }

    #[test]
    fn a_list_is_read_with_everything_it_asks_for() {
        let tree = split(
            "Our world.\n\n<list type=Place style=cards columns=5 show=\"image, Climate\" where=\"Role=NPC\" below=\"North America\" filter=Climate sort=\"Climate desc, name\" count=Mayor by=Climate add=new>\n</list>\n\nAfter.",
        );
        let list = tree
            .iter()
            .find(|n| matches!(n, Node::Listing { .. }))
            .unwrap_or_else(|| panic!("no list: {tree:?}"));
        assert_eq!(
            list,
            &Node::Listing {
                of: "Place".into(),
                style: "cards".into(),
                columns: 3,
                show: vec!["image".into(), "Climate".into()],
                filter: vec![("Role".into(), "NPC".into())],
                below: "North America".into(),
                from: String::new(),
                when: String::new(),
                choose: "Climate".into(),
                sort: vec!["Climate desc".into(), "name".into()],
                count: "Mayor".into(),
                by: "Climate".into(),
                add: "new".into(),
            }
        );
        // The closing tag is swallowed, not shown as text.
        assert!(!format!("{tree:?}").contains("</list>"), "{tree:?}");
        assert!(format!("{tree:?}").contains("After."));
    }

    #[test]
    fn a_table_is_a_style_and_bare_words_are_not() {
        let style = |text: &str| match split(text).first() {
            Some(Node::Listing { style, .. }) => style.clone(),
            other => panic!("no list: {other:?}"),
        };
        assert_eq!(style("<list type=Character style=table show=\"Status, Species\">"), "table");
        assert_eq!(style("<list type=Place cards>"), "list", "one way to say it: style=");
        assert_eq!(
            split("<create type=\"Event, Session log\" label=\"New event\">").first(),
            Some(&Node::Create { of: vec!["Event".into(), "Session log".into()], label: "New event".into() })
        );
        assert_eq!(style("<list type=Place style=nonsense>"), "list");
    }

    #[test]
    fn a_graph_reads_its_people_and_lines() {
        let tree = split(
            "Before.\n\n<graph type=Character via=\"Relations, Family\" around=self depth=9>\n<person who=Tim at=12,-6 color=rose sub=note note=\"The heir\">\n\n<person who=\"Anna Lee\" at=x,1>\n<line between=\"Tim, Anna Lee & Franklin\" color=amber sides=\"bottom 30, nowhere\" shape=corner arrow=both label=30 dock=\", 30\" group=\"nonsense, each\" say=\"Parents, \">\n<line between=Tim color=amber>\n<line between=\"A & B & C, D\">\n</graph>\nAfter.",
        );
        assert!(matches!(tree.first(), Some(Node::Paragraph { .. })), "{tree:?}");
        assert!(matches!(tree.last(), Some(Node::Paragraph { .. })), "{tree:?}");
        let Some(Node::Graph(g)) = tree.get(1) else { panic!("no graph: {tree:?}") };
        assert_eq!(g.of, "Character");
        assert_eq!(g.via, vec!["Relations".to_string(), "Family".to_string()]);
        assert_eq!((g.around.as_str(), g.depth), ("self", 5));
        assert_eq!(
            g.people,
            vec![GraphPerson { who: "Tim".into(), x: 12, y: -6, color: "rose".into(), sub: "note".into(), note: "The heir".into() }],
            "a place that does not read is left out"
        );
        assert_eq!(
            g.lines,
            vec![GraphLine {
                between: vec!["Tim".into(), "Anna Lee & Franklin".into()],
                color: "amber".into(),
                sides: vec!["bottom 30".into(), String::new()],
                shape: "corner".into(),
                arrow: "both".into(),
                dock: vec![String::new(), "30".into()],
                group: vec![String::new(), "each".into()],
                say: vec!["Parents".into(), String::new()],
                label: "30".into(),
            }],
            "a line between fewer than two ends, or an end of three, is left out"
        );
    }

    #[test]
    fn a_graph_without_its_end_is_the_line_alone() {
        let tree = split("<graph around=self>\nText after.");
        assert!(matches!(tree.first(), Some(Node::Graph(_))), "{tree:?}");
        assert!(format!("{tree:?}").contains("Text after."), "{tree:?}");
        assert!(split("<graphic>").iter().all(|n| !matches!(n, Node::Graph(_))));
    }

    #[test]
    fn two_lists_in_one_html_block_are_both_read() {
        let tree = split("<list type=Place>\n<list type=City columns=2>");
        let count = tree.iter().filter(|n| matches!(n, Node::Listing { .. })).count();
        assert_eq!(count, 2, "{tree:?}");
    }

    #[test]
    fn an_empty_condition_asks_for_an_empty_field() {
        let tree = split("<list type=Place where=\"Lies in=\">");
        let Some(Node::Listing { filter, .. }) = tree.first() else {
            panic!("no list: {tree:?}");
        };
        assert_eq!(filter, &vec![("Lies in".to_string(), String::new())]);
    }

    #[test]
    fn a_map_holds_its_pins() {
        let tree = split("Before.\n<map src=img:abc height=600>\n<pin at=34.5,62 label=\"Stair down\" to=Cellar color=amber>\n<pin at=120,x>\n<pin at=150,-3 label=Edge>\n</map>\nAfter.");
        let Some(Node::Map(m)) = tree.iter().find(|n| matches!(n, Node::Map(_))) else { panic!("{tree:?}") };
        assert_eq!(m.src, "img:abc");
        assert_eq!(m.height, 600);
        assert_eq!(
            m.pins,
            vec![
                MapPin { x: 345, y: 620, label: "Stair down".into(), to: "Cellar".into(), color: "amber".into(), icon: String::new(), ..Default::default() },
                MapPin { x: 1000, y: 0, label: "Edge".into(), to: String::new(), color: String::new(), icon: String::new(), ..Default::default() },
            ],
            "a pin that does not say where is left out; one beyond the edge stands on it"
        );
        assert!(format!("{tree:?}").contains("After."));
        let Some(Node::Map(bare)) = split("<map src=img:abc>\n</map>").into_iter().next() else { panic!() };
        assert_eq!(bare.height, MAP_HEIGHT);
        let Some(Node::Map(more)) = split(
            "<map src=img:abc>\n<pin at=1,2 icon=ladder>\n<pin at=1,2 icon=dragon>\n<area at=\"10,20 30,20 30,40\" label=North to=Northland color=green>\n<area at=\"10,20 30,20\">\n</map>",
        )
        .into_iter()
        .next() else { panic!() };
        assert_eq!(more.pins.iter().map(|p| p.icon.as_str()).collect::<Vec<_>>(), ["ladder", ""], "an unknown marker is the pin");
        assert_eq!(more.areas, vec![MapArea { points: vec![(100, 200), (300, 200), (300, 400)], label: "North".into(), to: "Northland".into(), color: "green".into(), ..Default::default() }], "a region needs three corners");
    }

    #[test]
    fn a_list_may_stand_in_a_line() {
        let tree = split("**Skills** <list from=Skills when=Proficiency style=inline show=\"[[+ Bonus]]\">\n**Senses** darkvision");
        let Node::Paragraph { children } = &tree[0] else { panic!("{tree:?}") };
        assert!(children.iter().any(|n| matches!(n, Node::Listing { style, .. } if style == "inline")), "{tree:?}");
    }

    #[test]
    fn a_list_goes_through_picks_and_keeps_what_its_formula_allows() {
        let tree = split("<list from=Classes.Features when=\"From level <= Level\" show=\"Level, [[if(Level > 2, 1d8, 1d6)]]\">");
        let Some(Node::Listing { from, when, show, .. }) = tree.first() else { panic!("{tree:?}") };
        assert_eq!(from, "Classes.Features");
        assert_eq!(when, "From level <= Level");
        assert_eq!(show, &vec!["Level".to_string(), "[[if(Level > 2, 1d8, 1d6)]]".to_string()]);
    }

    #[test]
    fn dice_in_text_become_rolls() {
        let tree = split("Hit: 7 (2d6 + 3) slashing, then 1W20-1 and d8, with advantage 2W20kh1 + 5. Not W3C or 2d6x.");
        let Node::Paragraph { children } = &tree[0] else { panic!("not a paragraph") };
        let rolls: Vec<&str> = children
            .iter()
            .filter_map(|n| match n {
                Node::Roll { text, .. } => Some(text.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(rolls, vec!["2d6 + 3", "1W20-1", "d8", "2W20kh1 + 5"]);
        assert!(format!("{tree:?}").contains("Not W3C or 2d6x."), "{tree:?}");
    }

    #[test]
    fn a_roll_in_brackets_keeps_its_formula() {
        let tree = split("Attack [[+4]] to hit, [[1d20 + Strength mod]].");
        let Node::Paragraph { children } = &tree[0] else { panic!("not a paragraph") };
        assert_eq!(children[1], Node::Roll { formula: "+4".into(), text: String::new() });
        assert_eq!(children[3], Node::Roll { formula: "1d20 + Strength mod".into(), text: String::new() });
        assert_eq!(children[4], text("."));
    }

    #[test]
    fn dice_inside_a_word_or_link_stay_text() {
        let tree = split("Room d12b and @@Place:Hall 2d6@@");
        assert!(!format!("{tree:?}").contains("Roll"), "{tree:?}");
    }

    #[test]
    fn a_rule_right_after_the_ability_row_stays_a_rule() {
        let tree = split("---\n<abilities>\n---\nAfter.");
        assert_eq!(tree.iter().filter(|n| matches!(n, Node::Rule)).count(), 2, "{tree:?}");
        assert!(matches!(tree[1], Node::Abilities { .. }), "{tree:?}");
    }

    #[test]
    fn a_view_names_its_group() {
        let tree = split("Before.\n<view of=Stats>\nAfter.");
        assert_eq!(tree[1], Node::View { of: "Stats".into() }, "{tree:?}");
    }

    #[test]
    fn abilities_name_their_properties() {
        let tree = split("<abilities show=\"Strength, Dexterity\">");
        assert_eq!(tree[0], Node::Abilities { show: vec!["Strength".into(), "Dexterity".into()], with: String::new(), short: vec![], also: String::new(), heading: String::new(), also_heading: String::new() });
        assert_eq!(split("<abilities>")[0], Node::Abilities { show: vec![], with: String::new(), short: vec![], also: String::new(), heading: String::new(), also_heading: String::new() });
        assert_eq!(
            split("<abilities show=Stats with=mod short=\"ST, GE\">")[0],
            Node::Abilities { show: vec!["Stats".into()], with: "mod".into(), short: vec!["ST".into(), "GE".into()], also: String::new(), heading: String::new(), also_heading: String::new() }
        );
    }

    #[test]
    fn empty_text_yields_nothing() {
        assert!(split("").is_empty());
        assert!(split("   \n\n  ").is_empty());
    }
}
