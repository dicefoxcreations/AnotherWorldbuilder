import { useState } from "react";

/**
 * The language of template definitions — for looking up and for inserting.
 *
 * Both from the same list: what stands in the right-click menu also stands
 * in the legend. Otherwise one place explains something the other does not
 * insert.
 *
 * On the left the example, on the right one sentence about it. Whoever looks
 * something up wants a line to copy, not an essay — the previous version had
 * three paragraphs per brick, and nobody reads that while typing.
 *
 * The bricks carry their placeholder in «guillemets». On insertion the
 * characters are removed and what stood between them is selected; in the
 * legend it is highlighted in colour. So the guillemets are a marker for us
 * and **never something anyone types**.
 */

export type Brick = {
  /** Short name — for the search and the right-click menu. */
  name: string;
  /**
   * Where it belongs.
   *
   * `head`: into the skeleton, not the body. `properties`: a line on top,
   * between `---`. `legend-only`: no brick to insert, but a value that
   * occurs in other tags. Only `head` and `legend-only` stay out of the
   * right-click menu.
   */
  where?: "head" | "properties" | "legend-only";
  /** The line as it is inserted. «…» marks the placeholder. */
  line: string;
  /** One sentence. No more. */
  what: string;
};

export type Group = { title: string; bricks: Brick[] };

export const BRICKS: Group[] = [
  {
    title: "Skeleton",
    bricks: [
      {
        name: "Skeleton",
        where: "head",
        line: "<article view=all edit=all>\n  <name>\n  <body>\n  </body>\n</article>",
        what: "Every type has it. You can change it, not delete it — it comes back.",
      },
      {
        name: "Permissions of the article",
        where: "head",
        line: "<article view=«players» edit=gm>",
        what: "Holds for everything that says nothing of its own. Every element may say view= and edit= — the innermost wins.",
      },
      {
        name: "Name from a pattern",
        where: "head",
        line: '<name="«Item» %%input%%">',
        what: "Without %%input%% nobody is asked for a name when creating one.",
      },
    ],
  },
  {
    title: "Properties — the data, on top",
    bricks: [
      {
        name: "Text",
        where: "properties",
        line: "«Title»:",
        what: "One line of text — nothing after the colon is text. A default goes after =: Mood: text = calm.",
      },
      {
        name: "Multiline",
        where: "properties",
        line: "«Motto»: textarea",
        what: "A short paragraph. Anything longer belongs in a block.",
      },
      {
        name: "Number",
        where: "properties",
        line: "«Age»: number years = 30",
        what: "What follows number is its unit and may be left out; after = the default.",
      },
      {
        name: "Reckoned number",
        where: "properties",
        line: "«Hit points»: number = [[«Class.Hit die + Level * mod(Constitution)»]]",
        what: "A default in [[…]] is a formula, not a value: a document without a value of its own has what it comes to — from its numbers and, over a dot, from what it picks (Class.Hit die). Feats add to it; a value of its own stands instead. A document's value may be a formula too: armour's [[14 + min(Dexterity mod, 2)]] reckons for whoever wears it."
      },
      {
        name: "Rule",
        where: "properties",
        line: "«mod»(«x») = «floor((x - 10) / 2)»",
        what: "A rule of your game, called in every formula like a function: mod(Strength), or Strength mod. Kept in a type or a group, it holds for all that take it in. check(x) is what [[+4]] and a click on a score roll. crit, fumble and success judge a check: a number (crit = 20: dice at 20 or more) or a rule of d, the dice, x, the value checked, and r, the result — success(d, x) = d <= x rolls under a skill. [[check(a, b, c, fw)]] calls a check of several values; shown says what its button shows. No game is built in."
      },
      {
        name: "World date",
        where: "properties",
        line: "«Born»: date",
        what: "A day in your world's calendar, with a time if wanted: 14 Mist Moon 203 AR, 18:30. The mark names the era. Options → Calendar sets eras, months and moons.",
      },
      {
        name: "Date and time",
        where: "properties",
        line: "«Played on»: datetime",
        what: "A day and time of ours — when a session was played.",
      },
      {
        name: "Pick a value",
        where: "properties",
        line: "«Gender»: @@Gender@@",
        what: "One of the documents of a type — its page names it. A data type makes it a plain choice. Default: = female.",
      },
      {
        name: "Pick several",
        where: "properties",
        line: "«Languages»: @@Language@@ multiple",
        what: "Like the above, only several at once. Default = *: every document of the type — all skills, to start with.",
      },
      {
        name: "Picks that add",
        where: "properties",
        line: "«Feats»: @@Feat@@ multiple adds",
        what: "What it picks changes this document's numbers, by name — as their operator says: Strength: +1 raises (a feat), -1 lowers, =19 sets, >=19 sets to at least 19 (gauntlets of ogre power), <=3 to at most 3. A bare number is a document's own value. Everywhere it shows and in every formula.",
      },
      {
        name: "Every one, with a number",
        where: "properties",
        line: "«Skills»: @@«Skill»@@ every with «Proficiency»",
        what: "Always every document of the type — none to add or take out, and one made later is there too. Only the numbers change; only those not 0 are written: @@Skill:Stealth@@ 2.",
      },
      {
        name: "What points here",
        where: "properties",
        line: "«Next»: @@«Session log»@@ from «Previous»",
        what: "Holds nothing of its own: the documents whose Previous names this one, as links. With = last on Previous, a new one takes the last by name — and the one before it gets its Next.",
      },
      {
        name: "Picks with a number",
        where: "properties",
        line: "«Classes»: @@«Class»@@ multiple with «Level»",
        what: "Each pick carries a number of its own — the level in each class, how many potions. The document writes it after the link: @@Class:Fighter@@ 3. In formulas: Classes.Level adds them up, Classes.(Level * Hit die) reckons within each pick, first(…) takes the first.",
      },
      {
        name: "Property group",
        where: "properties",
        line: "«Stats»: group Abilities",
        what: "The properties of a group (Options › Types › Property groups), as if they were this type's own. What changes in the group changes in every type that takes it in. Without a name after group, the group called like the property. A group may take in groups, and may have a layout below its properties.",
      },
      {
        name: "Parent",
        where: "properties",
        line: "«Lies in»: parent",
        what: "Where it sits in the tree of its own type: breadcrumbs and tree view come from it. One per type.",
      },
      {
        name: "Parent of another type",
        where: "properties",
        line: "«Belongs to»: @@Era@@ parent",
        what: "When the tree crosses types: the parent is a document of that type.",
      },
      {
        name: "Relations",
        where: "properties",
        line: "«Relations»: @@Character@@ as @@Relation@@",
        what: "People with a role each side: @@Olga@@ Mother/Daughter. Written into both documents. view=gm keeps them secret.",
      },
      {
        name: "Image",
        where: "properties",
        line: "«Portrait»: image 200x260",
        what: "A size, and left, center, right or none.",
      },
      {
        name: "Secret property",
        where: "properties",
        line: "«True goal»: text view=gm edit=gm",
        what: "Who may see and change it. A layer of its own — whoever may not does not even see its name.",
      },
    ],
  },
  {
    title: "Infobox — what it shows",
    bricks: [
      {
        name: "Infobox",
        line: "<infobox align=«right»>\n</infobox>",
        what: "The box beside the text. align: right, left, top, none. width: auto, narrow, wide or pixels.",
      },
      {
        name: "Show a text",
        line: "<text bind=«Title»>",
        what: "Shows a property; while editing it is a field. textarea for a paragraph. label= to call it otherwise.",
      },
      {
        name: "Show a choice",
        line: "<select bind=«Gender»>",
        what: "For a property that picks documents — a dropdown while editing.",
      },
      {
        name: "Show a number",
        line: "<number bind=«Age»>",
        what: "date for a world date.",
      },
      {
        name: "Show an image",
        line: "<image bind=«Portrait»>",
        what: "Stands on top of the box, without a label.",
      },
    ],
  },
  {
    title: "Blocks",
    bricks: [
      {
        name: "Text block",
        line: '<block title="«What happened»">\n</block>',
        what: "Flowing text in the <body>. Several of them read as one.",
      },
      {
        name: "Box",
        line: '<block title="«House rule»" style=box>\n</block>',
        what: "Set apart with a frame. style: box or quote.",
      },
      {
        name: "Stat block",
        line: '<block title="«Stats»" style=statblock>\n</block>',
        what: "Values at a glance, as a card: # is the name, ## a section like Actions, --- a rule.",
      },
      {
        name: "Sheet on top, folded",
        line: '<block title="«Sheet»" style=statblock top fold=folded view="self OR gm">\n</block>',
        what: "top: above the infobox, page-wide. fold=folded starts closed, fold=off never folds. self: whoever is assigned to the document.",
      },
      {
        name: "Smaller title",
        line: '<block title="«Notes»" heading=3>\n</block>',
        what: "1 is as large as # in the text (the default), 4 the size of the text.",
      },
      {
        name: "Block below, page-wide",
        line: '<block title="«Family»" bottom>\n</block>',
        what: "Under the infobox and the text, across the page — for family trees and timelines.",
      },
      {
        name: "Block at the side",
        line: '<block title="«Timeline»" side width=«380»>\n</block>',
        what: "A column of its own beside the text, width in pixels. Its title folds it away sideways.",
      },
      {
        name: "Order of the blocks",
        line: '<order of="«Goals», «Appearance»">',
        what: "Inherited blocks too, by title. Those not named follow. In the editor: drag a block by its title.",
      },
      {
        name: "Order of the documents",
        line: '<sort by="«Date», name">',
        what: "In lists and the outline, by these properties, then by name. Subtypes sort the same.",
      },
      {
        name: "Block with its own permissions",
        line: '<block title="«Secret»" view=gm edit=gm>\n</block>',
        what: "Holds for the whole block. Without it, the article's hold.",
      },
    ],
  },
  {
    title: "In the text",
    bricks: [
      {
        name: "Reference",
        line: "@@«Place:Morningstone»@@",
        what: "A link to a document: type, colon, name, closed by a second @@. Typing @@ suggests them. Other text after a bar: @@Species:Elves|Elf@@.",
      },
      {
        name: "Property of another document",
        line: "@@«Character:Tim»#Status@@",
        what: "Shows the current value of that document's property. Typing # after a document suggests them.",
      },
      {
        name: "Value of a property",
        line: "%%«Title»%%",
        what: "Reads a property of the document right there in the text.",
      },
      {
        name: "Property in the text",
        line: "<select bind=«Gender»>",
        what: "Like in the infobox: shows the value, and is a field while editing.",
      },
      {
        name: "Image in the text",
        line: '<image size=«300x200» align=right caption="The shire">',
        what: "With align=left and align=right the text flows around it.",
      },
      {
        name: "Region with its own permissions",
        line: "<block view=«gm» edit=gm>\n</block>",
        what: "A block inside a block: cuts a piece out of the text and gives it its own permissions.",
      },
      {
        name: "Element with its own permissions",
        line: "<image size=«300x200» view=gm edit=gm>",
        what: "An image, a list in the text may say view= and edit=. Then only they get it. A property says it on top.",
      },
      {
        name: "Hidden image with a stand-in",
        line: '<image src=«img:…» view=gm edit=gm hidden=«img:…» hidden-caption="A stranger">',
        what: "Who may not see it sees the stand-in and its caption instead. Change view= to reveal it.",
      },
      {
        name: "Roll",
        line: "[[«1d20 + Strength mod»]]",
        what: "A click rolls it into the tray. Dice as written in the text roll too: 2d6 + 3, 1W20. Names are this document's numbers; Strength mod calls the rule mod on Strength.",
      },
      {
        name: "Check",
        line: "[[«+4»]]",
        what: "Shows +4 and rolls the rule check on it: with check(x) = 1d20 + x, 1d20 + 4 — “+4 to hit”. Also [[+ Strength mod]]. Without a check rule it is just +4.",
      },
      {
        name: "Reckon",
        line: "[[«10 + mod(Dexterity)»]]",
        what: "Without dice it shows the number. + - * /, brackets, comparisons (< <= > >= = !=, 1 or 0), if(c, a, b), and, not, floor, ceil, round, min, max, steps(Level, 1, 2, 5, 3) for a table by steps — and your rules. 2W20kh1 keeps the highest die, 4d6kh3 the three highest, 2W20kl1 the lowest. Classes[Class].Level: of the picks, those another reference picks — or the one so named. Each die: map(rule, 5W6) takes a rule to every die, [a, b, c] is a list, sum/count/min/max take a list value by value — pools and three dice against three values.",
      },
      {
        name: "Ability row",
        line: '<abilities show=«Abilities» with=«mod» short="«STR, DEX, CON, INT, WIS, CHA»">',
        what: "The numbers of a group, or names listed. with= names the rule whose result stands below each score; a click rolls the rule check on it. short= heads the tiles, otherwise their first three letters. also=\"Saving throws.Bonus\": a second line, under each score the pick named like it. heading= and also-heading= say what the lines are.",
      },
      {
        name: "Group layout",
        line: "<view of=«Charakterwerte»>",
        what: "The layout a property group defines, drawn with this document's values — kept once, in the group, and the same wherever it is taken in. %%Name%% in it is the document's name; <abilities show=«Group»> shows a group's numbers.",
      },
      { name: "Note", line: "<!-- «Note to self» -->", what: "Skipped over." },
    ],
  },
  {
    title: "Lists — in any text",
    bricks: [
      {
        name: "List",
        line: '<list type="«Place»" columns=2>',
        what: "Every document of a type and its subtypes, by name. columns: 1 to 3 — only for this plain list.",
      },
      {
        name: "Cards",
        line: '<list type="«Place»" style=cards show="image, Climate">',
        what: "The documents as small cards. show: image and properties.",
      },
      {
        name: "Table",
        line: '<list type="«Character»" style=table show="Status, Species">',
        what: "One row per document: the name, then a column for each property in show.",
      },
      {
        name: "Tree",
        line: '<list type="«Place»" style=tree>',
        what: "Along the parent property — what lies in what.",
      },
      {
        name: "From a property",
        line: '<list from="«Equipment»" style=table show="Rarity">',
        what: "What a property of this document picks (@@Item@@ multiple), in its order. Whoever may change the document adds with ＋ and removes with ×, right in the list.",
      },
      {
        name: "Through the picks",
        line: '<list from="«Classes.Features»" when="«From level <= Level»" show="«[[+ Bonus]]»">',
        what: "A path goes on through what is picked: the features each class brings. when: a formula, reckoned inside each — what it makes 0 is left out. show may hold the pick's own number (Proficiency), numbers reckoned for whoever picked it, and rolls in [[…]]. A plain list shows them after the name. ●Proficiency shows a number as so many marks before it.",
      },
      {
        name: "In the line",
        line: '**«Skills»** <list from=«Skills» when=«Proficiency» style=inline show="«[[+ Bonus]]»">',
        what: "The list by commas, where it stands in the text — a short stat block: Stealth +6, Perception +2.",
      },
      {
        name: "Timeline",
        line: '<timeline type="«Event, Session»">',
        what: "Every date of these documents along the world's calendar — births, deaths, sessions, events. around=self: only what names this document. show=: only these date properties. until=End: that date ends a span instead of standing on its own — several by comma, one for each type. text=\"Summary, Course\": an entry shows the first of these parts — or text properties — its document has, else its first text; text=none: no text. A document without a date stands on the date of what it picks — a kill on its session's; on one day the first type named leads, with what belongs to it after it. ⚙ sets all of it by click.",
      },
      {
        name: "Calendar",
        line: '<calendar type="«Session, Event»">',
        what: "A month of the world's calendar with its moons, and what of these documents happened on its days. It opens at the latest; ‹ › turn the months.",
      },
      {
        name: "Relation tree",
        line: "<graph>\n</graph>",
        what: "A surface of people and their relations. Everything can be set by clicking it while editing: a card, a line, the free ground. Without more it shows only whom you add; around=self depth=2: also the relatives of this document; type=: everyone of a type.",
      },
      {
        name: "Map",
        line: '<map src=«img:…» height=«480»>\n<pin at=«30,40» label="«Stair down»" to=«Cellar» color=«amber» icon=«stairs»>\n<area at="«10,20 30,20 30,40»" label=«Northland» to=«Northland»>\n</map>',
        what: "A picture to drag and zoom — wheel, double-click, + − ⤢. Markers lie where they lie, in percent; icon: door, ladder, stairs, chest, skull, castle, tower, house, camp, cave, ship, anchor, tree, mountain, flag, star, sword, eye, question. A region is a shape around a part of the picture that lights up under the pointer. Both may lead to a document; one with a map of its own is gone to in place, ‹ comes back. While writing: ＋ Marker and a click; ＋ Region and a click at each corner, the first again to close it; drag a marker or a corner to move it.",
      },
      {
        name: "Create button",
        line: '<create type="«Event, Session log»" label="«New event»">',
        what: "A button that makes a new document of the type — of several, a click asks which. It shows the types one may make: the GM all, players those whose <article> says create=. Without label: ＋ New and the type.",
      },
      {
        name: "Players make documents",
        line: '<article view=«players» edit=«players» create=«players»>',
        what: "create= on a type: who besides the GM may make its documents — players, class:rogue, … A player's new document goes to the GM's app, which makes it with the type's access.",
      },
      {
        name: "Secret marker, its name below",
        line: '<pin at=«30,40» label=«Cave» label-at="«bottom hover»" view=«gm» edit=«gm»>',
        what: "label-at: left, top, bottom — right without it; with hover only under the pointer. view= on a marker or region: only they have it — reveal it by widening view, or remove it to show it with the map. Documents only, not types.",
      },
      {
        name: "Inside a graph",
        where: "legend-only",
        line: '<person who=«Tim» at=12,6 color=amber sub=note note="The heir">\n<line between="Simon & Anna, Franklin & Tim" dock="50, 40" say="Parents, Sons">',
        what: "Written by clicking and dragging. color: teal, amber, blue, rose, green, violet, red, cyan, grey. sub: type, role, note, none. sides: a side and how far along it in percent. shape: curve, straight, corner. arrow: end, start, both. label: how far the roles stand from their people (5–50), or none. between: each end someone, or the line between two (Anna & Franklin) — everyone at one end related to everyone at the other, drawn as one. dock: how far along such an end's line. group: each — each one's role at such an end, instead of the roles once. say: a word of one's own in place of their roles (Parents). On <graph>: show=\"portrait, type, roles\".",
      },
      {
        name: "Filtered",
        line: '<list type="Character" where="«Role=NPC»">',
        what: "Only documents whose property has this value. below=Name: only what lies below it.",
      },
      {
        name: "Filter buttons",
        line: '<list type="Character" filter="«Role»">',
        what: "A button for each value above the list; the reader picks one, or All.",
      },
      {
        name: "Order of a list",
        line: '<list type="Character" style=table sort="«Role» desc, name">',
        what: "How it starts; desc turns it round. A click on a column's head sorts by it, ↺ goes back.",
      },
      {
        name: "Count",
        line: '<list type="Kill" count="«Killer»" by="«Kind»">',
        what: "A row for each value and how many have it, most first. by: a column for each of its values too.",
      },
      {
        name: "Another page",
        line: "<embed of=«Sessions»>",
        what: "Its text here, under its name. A type's page brings its ＋.",
      },
    ],
  },
  {
    title: "Values",
    bricks: [
      {
        name: "view, edit",
        where: "legend-only",
        line: "view=«players» edit=gm",
        what: "all, players, gm, an attribute like class:rogue — several with OR.",
      },
      {
        name: "%%input%%, %%count%%",
        where: "legend-only",
        line: '<name="Session log «%%count%%»">',
        what: "%%input%% is what is typed, %%count%% counts up. Same meaning everywhere.",
      },
      {
        name: "type",
        where: "legend-only",
        line: "type=«Place»",
        what: "The name of a type as it stands in the options. Rename it and the new one appears here on the next save.",
      },
    ],
  },
];

/** Shows a line and highlights the placeholder. */
function Line({ text }: { text: string }) {
  const open = text.indexOf("«");
  const close = text.indexOf("»");
  if (open < 0 || close < open) return <code>{text}</code>;
  return (
    <code>
      {text.slice(0, open)}
      <span className="placeholder">{text.slice(open + 1, close)}</span>
      {text.slice(close + 1)}
    </code>
  );
}

/**
 * The legend.
 *
 * Stands in a window of its own that can be pushed to the edge of the
 * screen — looking up and typing at the same time only works that way.
 */
export function Legend() {
  const [search, setSearch] = useState("");
  const [closed, setClosed] = useState<Record<string, boolean>>({});

  const matching = (g: Group) => {
    const s = search.trim().toLowerCase();
    if (!s) return g.bricks;
    return g.bricks.filter((b) => `${b.name} ${b.what} ${b.line}`.toLowerCase().includes(s));
  };

  return (
    <div className="legend">
      <header>
        <h2>How a template is written</h2>
        <p className="small">
          Tags as in HTML. <span className="placeholder">Coloured</span> is what you replace;
          everything else stands exactly as written. Values with spaces go in{" "}
          <code>"quotes"</code>.
        </p>
        <input
          className="legend-search"
          value={search}
          onChange={(e) => setSearch(e.target.value)}
          placeholder="Search — “link” or “permissions”, say"
        />
      </header>

      {BRICKS.map((group) => {
        const hits = matching(group);
        if (hits.length === 0) return null;
        const collapsed = closed[group.title] && !search.trim();
        return (
          <section key={group.title} className={collapsed ? "group collapsed" : "group"}>
            <button
              className="group-head"
              onClick={() => setClosed((z) => ({ ...z, [group.title]: !z[group.title] }))}
              aria-expanded={!collapsed}
            >
              <span className="arrow">{collapsed ? "▸" : "▾"}</span>
              {group.title}
              <small>{hits.length}</small>
            </button>

            {!collapsed && (
              <div className="legend-table">
                {hits.map((b) => (
                  <div key={b.name} className="legend-row">
                    <Line text={b.line} />
                    <p>{b.what}</p>
                  </div>
                ))}
              </div>
            )}
          </section>
        );
      })}

      <p className="small footnote">Right-click in the text field inserts most of these lines.</p>
    </div>
  );
}
