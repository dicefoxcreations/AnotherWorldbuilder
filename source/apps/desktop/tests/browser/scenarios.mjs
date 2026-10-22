/**
 * Scenarios at the test bench: what someone does, not what the code does.
 *
 * The Rust tests check rules. This file holds what comes out of using the
 * app — with real keystrokes, real clicks and the real Rust core behind
 * them. Every scenario ends with a claim; if it does not hold, the scenario
 * aborts and says what it found instead.
 *
 * ```text
 * cargo run -p aw-testbench &         # the core
 * npm run testbench &                 # the page
 * google-chrome-stable --headless=new --remote-debugging-port=9222 \
 *   --user-data-dir=/tmp/aw-chrome about:blank &
 * node tests/browser/scenarios.mjs        # all
 * node tests/browser/scenarios.mjs typing # one
 * ```
 */

import { Page } from "./driver.mjs";

const BASE = process.env.AW_TESTBENCH ?? "http://localhost:5321";

function check(claim, actual, expected) {
  const same = JSON.stringify(actual) === JSON.stringify(expected);
  console.log(`  ${same ? "✓" : "✗"} ${claim}`);
  if (!same) {
    console.log(`    expected: ${JSON.stringify(expected)}`);
    console.log(`    got:      ${JSON.stringify(actual)}`);
    process.exitCode = 1;
  }
}

const SCENARIOS = {
  /** A new document: its texts and its image fields take images — a player's waiting one too. */
  async create(page) {
    await page.openStage("create");
    await page.settle(800);
    check("the form shows without an error", await page.js(`return document.body.innerText.includes("TypeError");`), false);
    await page.js(`[...document.querySelectorAll(".block-tools button")].find((b) => /image/i.test(b.title)).click(); return 1;`);
    await page.settle(300);
    check("a text takes an image — upload or gallery", await page.js(`return !!document.querySelector(".image-picker");`), true);
    await page.key("Escape");
    await page.js(`document.querySelector(".image-picker .ghost, .image-picker button")?.click(); return 1;`);
    await page.settle(200);
    await page.js(`document.querySelector(".infobox button.choosable").click(); return 1;`);
    await page.settle(300);
    check("an image field takes one too", await page.js(`return !!document.querySelector(".image-picker");`), true);
    await page.key("Escape");
    await page.js(`document.querySelector(".image-picker")?.querySelector("button.ghost, .close")?.click(); return 1;`);
    await page.settle(200);

    // Who sees it: ＋ offers the basics, and a field finds the rest by name.
    await page.js(`document.querySelector(".article-head .bubble.plus").click(); return 1;`);
    await page.settle(200);
    check("＋ offers the basics — no long lists, no switch", await page.js(`const m = document.querySelector(".view-menu"); return [[...m.querySelectorAll(":scope > button")].map((b) => b.textContent), !!m.querySelector(".view-search"), m.textContent.includes("advanced")];`), [["All players", "GM only"], true, false]);
    await page.js(`const i = document.querySelector(".view-search"); Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value").set.call(i, "charac"); i.dispatchEvent(new Event("input", { bubbles: true })); return 1;`);
    await page.settle(150);
    check("typing finds a type", await page.js(`return [...document.querySelectorAll(".view-found button")].map((b) => b.textContent);`), ["All CharacterType"]);
    await page.js(`document.querySelector(".view-search").dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true })); return 1;`);
    await page.settle(150);
    check("Enter adds it — beside the GM; everyone goes, it would take everyone in", await page.js(`return [...document.querySelectorAll(".article-head .bubble:not(.plus)")].map((b) => b.textContent.replace("×", ""));`), ["GM", "All Character"]);
  },

  /**
   * Writing in the body. A half-typed `<` used to catch the `>` of the next
   * line, so the skeleton counted as deleted and the editor wrote everything
   * afresh — including the character that caused it.
   */
  async typing(page) {
    await page.openStage("blank");
    await page.caretAfter(".template-text", "<body>");
    await page.key("Enter");
    // The editor closes the block itself and puts the caret inside.
    await page.type('      <block title="History">');
    await page.type("The party set out.");
    await page.settle(600);

    const text = await page.text(".template-text");
    check("what was typed stands in the body", text.includes('<block title="History">'), true);
    check("the skeleton is intact", text.includes("</body>") && text.includes("</article>"), true);
    check(
      "the preview shows the block",
      await page.js(
        `return [...document.querySelectorAll(".preview-side .block-title")].map((b) => b.textContent);`,
      ),
      ["History"],
    );
    check(
      "and there is no error",
      await page.js(`return document.querySelectorAll(".error-list li").length;`),
      0,
    );
  },

  /**
   * The skeleton cannot be deleted — but it must not take away anything
   * someone is writing right now.
   */
  async skeleton(page) {
    await page.openStage("blank");
    await page.js(`
      const f = document.querySelector(".template-text");
      const setValue = Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, "value").set;
      setValue.call(f, "<article view=all edit=all>\\n  <name>\\n</article>\\n");
      f.dispatchEvent(new Event("input", { bubbles: true }));
    `);
    await page.settle(700);
    const text = await page.text(".template-text");
    check("the missing <body> comes back", text.includes("<body>"), true);
    check(
      "and the editor says so",
      await page.js(`return !!document.querySelector(".repaired-note");`),
      true,
    );
  },

  /** A brick lands in the body even when the cursor stands in the head. */
  async inserting(page) {
    await page.openStage("blank");
    await page.caretAfter(".template-text", "<article view=all edit=all>");
    await page.click(".template-text", { right: true });
    await page.settle(300);
    await page.clickText(".brick-menu button", "Text block");
    await page.settle(600);

    const lines = (await page.text(".template-text")).split("\n");
    const where = lines.findIndex((z) => z.includes("<block"));
    const bodyOpen = lines.findIndex((z) => z.includes("<body>"));
    const bodyClose = lines.findIndex((z) => z.includes("</body>"));
    check(
      "the block stands between <body> and </body>",
      where > bodyOpen && where < bodyClose,
      true,
    );
  },

  /** Switching the right side to the documents leaves the text standing. */
  async switching(page) {
    await page.openStage("template");
    const before = await page.text(".template-text");

    await page.clickText(".editor-head .segmented button", "Documents");
    await page.settle(500);
    check(
      "the documents now stand on the right",
      await page.js(`return !!document.querySelector(".value-list");`),
      true,
    );
    check("the text is untouched", await page.text(".template-text"), before);

    await page.clickText(".editor-head .segmented button", "Preview");
    await page.settle(500);
    check(
      "and back, the preview stands there again",
      await page.js(`return !!document.querySelector(".preview-side .article-page");`),
      true,
    );
    check("the text still untouched", await page.text(".template-text"), before);
  },

  /**
   * Renaming. The id does not stand in the text — so a reference may carry
   * the new name without anything being rehung.
   */
  async renaming(page) {
    await page.openStage("template");
    const before = await page.text(".template-text");
    if (!before.includes("@@Gender@@")) {
      throw new Error(`The type name is not in the text:\n${before}`);
    }

    // Rename the type “Gender” — in the app Rust does that on save, here
    // the page does.
    await page.js(`
      window.setDirectory([
        { id: "character", name: "Character" },
        { id: "gender", name: "Gender identity" },
      ]);
      return true;
    `);

    // Change something about the access: that writes the text afresh from
    // the structure — and the new name has to appear in the process.
    await page.choose(".article-meta .level-picker", "sealed");
    await page.settle(900);

    const after = await page.text(".template-text");
    check(
      "the reference carries the new name",
      after.includes("@@Gender identity@@"),
      true,
    );
    check("and no longer the old one", after.includes("@@Gender@@"), false);
  },

  /** The types overview lists every type; subtypes fold away inside their supertype. */
  async types(page) {
    await page.openStage("options");
    check(
      "both types stand there",
      await page.js(`return [...document.querySelectorAll(".type-row strong")].map((s) => s.textContent);`),
      ["Character", "Gender"],
    );
    await page.js(`document.querySelector(".type-fold:not(:disabled)").click(); return true;`);
    await page.settle(200);
    check(
      "unfolded, the NPC stands inside Character",
      await page.js(`return [...document.querySelectorAll(".type-row strong")].map((s) => s.textContent);`),
      ["Character", "NPC", "Gender"],
    );
  },

  /**
   * A block typed right before `</body>`: its closing tag comes along, and
   * `</body>` keeps a line of its own — it used to end up behind `</block>`,
   * and the skeleton counted as broken.
   */
  async closing(page) {
    await page.openStage("blank");
    // The caret at the start of the line with </body>.
    await page.caretAfter(".template-text", "<body>\n");
    await page.js(`
      const f = document.querySelector(".template-text");
      const at = f.value.indexOf("</body>");
      const start = f.value.lastIndexOf("\\n", at) + 1;
      f.setSelectionRange(start, start);
      return true;`);
    await page.type("<block>");
    await page.type("Inside.");
    await page.settle(600);
    const text = await page.text(".template-text");
    check("</body> stands on a line of its own", /\n\s*<\/body>/.test(text) && !/<\/block>\s*<\/body>/.test(text.split("\n").find((l) => l.includes("</body>"))), true);
    check("the text went inside the block", /<block>\s*\n\s*Inside\./.test(text), true);
    check(
      "and there is no error",
      await page.js(`return document.querySelectorAll(".error-list li").length;`),
      0,
    );
  },

  /**
   * Relations as a tree: parents above with arrows down to their children,
   * someone unreadable a `???` of their own, and a person added from the
   * graph gets a place in it.
   */
  async stats(page) {
    await page.openStage("stats");
    await page.settle(600);
    const row = (where) => page.js(`return [...document.querySelectorAll("${where} .ability")].map((a) => a.innerText.replace(/\\s+/g, " ").trim());`);
    check("the row: the group's scores, below each what the world's rule mod makes of it", await row("#goblin"), [
      "STR 8 −1", "DEX 14 +2", "CON 10 +0", "INT 10 +0", "WIS 8 −1", "CHA 8 −1",
    ]);
    check("gauntlets set Strength to 19, the feat's +1 does not lift it — and below, its saving throw", (await row("#brakka"))[0], "STR 19 +4");
    check("a feat adds to Constitution, everywhere", (await row("#brakka"))[2], "CON 16 +3");
    check("over the scores and over their saves: what they are", await page.js(`return [...document.querySelectorAll("#brakka .ability-caption")].map((c) => c.textContent);`), ["Abilities", "Saves"]);
    check(
      "saving throws under their scores (Strength, Constitution, Wisdom here), proficient or not",
      await page.js(`return [...document.querySelectorAll("#brakka .ability-also")].map((a) => (a.querySelector(".ability-marks .dot.none") ? "" : "●") + a.innerText.replace(/[●\\s]+/g, ""));`),
      ["●+6", "●+5", "+1"],
    );
    check(
      "a sheet on top stands above the infobox and the text, across the page",
      await page.js(`const top = document.querySelector("#brakka .block.at-top"); const body = document.querySelector("#brakka .article-body"); return !!top && !!(top.compareDocumentPosition(body) & Node.DOCUMENT_POSITION_FOLLOWING) && top.getBoundingClientRect().width > body.querySelector(".flowing-text").getBoundingClientRect().width;`),
      true,
    );
    check(
      "a block placed bottom stands below the infobox and the text, across the page",
      await page.js(`const low = document.querySelector("#brakka .block.at-bottom"); const body = document.querySelector("#brakka .article-body"); return !!low && !!(body.compareDocumentPosition(low) & Node.DOCUMENT_POSITION_FOLLOWING) && low.getBoundingClientRect().top >= body.getBoundingClientRect().bottom && low.getBoundingClientRect().width > body.querySelector(".flowing-text").getBoundingClientRect().width && low.innerText.includes("Thráin");`),
      true,
    );
    await page.js(`document.querySelector("#brakka .block.at-top .block-fold").click(); return 1;`);
    await page.settle(100);
    check("its title folds it", await page.js(`return [document.querySelector("#brakka .block.at-top").classList.contains("folded"), document.querySelectorAll("#brakka .block.at-top .ability").length];`), [true, 0]);
    await page.js(`document.querySelector("#brakka .block.at-top .block-fold").click(); return 1;`);
    await page.settle(100);
    check("and opens it again", await page.js(`return document.querySelectorAll("#brakka .block.at-top .ability").length;`), 6);

    // A heading in the text folds what follows it — up to the next as high,
    // on into the next part.
    const story = () => page.js(`const b = [...document.querySelectorAll("#brakka .block.text")].find((b) => b.innerText.includes("Youth")); const access = b.querySelector(".head-access")?.innerText.split("\\n") ?? []; return b.innerText.split("\\n").filter((l) => l.trim() && l !== "⧉" && !access.includes(l));`);
    check(
      "beside a section's title, who views and changes it — only where that is not the document's",
      await page.js(`return [...document.querySelectorAll("#brakka .block-mark")].filter((m) => m.querySelector(".head-access")).map((m) => [m.querySelector(".block-title").textContent.trim(), m.querySelector(".head-access").innerText.replace(/\\s+/g, " ")]);`),
      [["Family", "Players"]],
    );
    const head = (name) => page.js(`[...document.querySelectorAll("#brakka .markdown-head.foldable")].find((h) => h.textContent === ${JSON.stringify(name)}).click(); return 1;`);
    check("a text's headings, all open", await story(), ["Story", "Youth", "Born under the mountain.", "Exile", "The Blue Mountains.", "Deeds", "Retook Erebor.", "Hid the map."]);
    await head("Youth");
    await page.settle(100);
    check("a heading folds up to the next as high", await story(), ["Story", "Youth", "Deeds", "Retook Erebor.", "Hid the map."]);
    await head("Youth");
    await head("Deeds");
    await page.settle(100);
    check("and on into the next part", await story(), ["Story", "Youth", "Born under the mountain.", "Exile", "The Blue Mountains.", "Deeds"]);
    await head("Deeds");
    await page.settle(100);
    check("the sheet is the group's layout, with Brakka's values", await page.js(`return document.querySelector("#brakka .group-view h2")?.textContent;`), "Brakka");
    const sheet = await page.js(`return document.querySelector("#brakka .group-view").innerText;`);
    check("hit points: the first class's die, then each class's average", /Hit points 39/.test(sheet), true);
    check("each class with its level", /Classes Fighter 3 Rogue 1/.test(sheet.replace(/\s+/g, " ")), true);
    check("speed from the species", /Speed 7.5 m/.test(sheet), true);
    check("a proficiency bonus by total level", /Proficiency bonus \+2/.test(sheet), true);
    check("passive perception through the skill so named", /Passive perception 15/.test(sheet), true);
    const listed = (n) =>
      page.js(`return [...document.querySelectorAll("#brakka .group-view .listing")][${n}].innerText.replace(/[×\\s]+/g, " ").trim();`);
    check("skills: every one, by name — proficient, expertise, not", await listed(0), "Athletics +6 Perception +5 Stealth +2");
    check("features as far as each class level reaches — no extra attack at fighter 3", await listed(1), "Fighting style Second wind Sneak attack");
    check("a subclass's features by the level in its class", await listed(2), "Improved critical");
    check(
      "proficiency as marks: proficient, expertise, half",
      await page.js(`return [...[...document.querySelectorAll("#brakka .group-view .listing")][0].querySelectorAll(".listing-marks")].map((m) => m.querySelector(".dot.none") ? "-" : [...m.querySelectorAll(".dot")].map((d) => (d.classList.contains("half") ? "◐" : "●")).join(""));`),
      ["●", "●●", "◐"],
    );
    check("a sheet of a group the goblin has not stays empty", await page.js(`return document.querySelectorAll("#goblin .group-view").length;`), 0);
    check(
      "a short stat block: the goblin's skills in the line, from its data",
      await page.js(`return [...document.querySelectorAll("#goblin p")].map((p) => p.innerText.split("\\n")[0]).find((t) => t.startsWith("Skills"));`),
      "Skills Stealth +6",
    );
    check("every skill, none to take out", await page.js(`return document.querySelectorAll("#brakka .group-view .listing .pick-x").length;`), 0);
    check(
      "a die reckoned from level and class, then the saves and skills",
      await page.js(`return [...document.querySelectorAll("#brakka .group-view button.roll")].map((b) => b.textContent);`),
      ["3d10 + 1d8 + 12", "+6", "+5", "+2"],
    );
    // Resting on a link shows what it links to.
    await page.js(`
      const link = [...document.querySelectorAll("#brakka .group-view .wikilink")].find((b) => b.textContent === "Ogre gauntlets");
      link.dispatchEvent(new MouseEvent("mouseover", { bubbles: true }));
      return 1;`);
    await page.settle(700);
    const preview = await page.js(`return document.querySelector(".link-preview")?.innerText.replace(/\\s+/g, " ").trim();`);
    check("a link previews its document — the page as it reads", /Ogre gauntlets/.test(preview ?? "") && /≥ 19/.test(preview ?? ""), true);
    check("only to look at: nothing to change in it", await page.js(`return document.querySelectorAll(".link-preview .visibility, .link-preview .delete, .link-preview .backlinks").length;`), 0);
    // A link in the preview opens the next one, above it.
    await page.js(`
      const link = [...document.querySelectorAll(".link-preview .wikilink")].find((b) => b.textContent === "Athlete");
      link.dispatchEvent(new MouseEvent("mouseover", { bubbles: true }));
      return 1;`);
    await page.settle(700);
    check(
      "a preview in the preview",
      await page.js(`return [...document.querySelectorAll(".link-preview")].map((c) => c.querySelector("h1")?.textContent.trim());`),
      ["🧤Ogre gauntlets", "⭐Athlete"],
    );
    await page.js(`document.body.dispatchEvent(new MouseEvent("mouseover", { bubbles: true })); return 1;`);
    check(
      "and says where it comes from",
      await page.js(`return [...document.querySelectorAll("#brakka .summed")].map((s) => s.title);`),
      ["39 by formula", "7.5 by formula", "2 by formula", "15 + 1 Athlete, at least 19 Ogre gauntlets", "12 + 2 Dwarf + 2 Tough"],
    );
    check("a check shows its bonus", await page.js(`return [...document.querySelectorAll("#goblin button.roll")].map((b) => b.textContent);`), [
      "2d6", "+6", "+4", "1d6 + 2", "+4", "1d6 + 2",
    ]);
    check("a sum without dice is its number", await page.js(`return document.querySelector("#goblin .reckoned")?.textContent;`), "9");
    await page.js(`return [...document.querySelectorAll("#goblin button.roll")].find((b) => b.textContent === "1d6 + 2").click();`);
    await page.settle(200);
    const entry = await page.js(`
      const e = document.querySelector(".dice-entry");
      return e && { label: e.querySelector(".dice-label").textContent, total: Number(e.querySelector(".dice-total").textContent) };`);
    check("a roll lands in the tray, named by its line", entry?.label, "Scimitar");
    check("between 3 and 8", entry && entry.total >= 3 && entry.total <= 8, true);
    // A skill's bonus rolls a check, named by the skill.
    await page.js(`return [...document.querySelectorAll("#brakka .group-view .listing")][0].querySelector("button.roll").click();`);
    await page.settle(200);
    check(
      "a skill rolls the rule check on its bonus",
      await page.js(`const e = document.querySelectorAll(".dice-entry"); const last = e[e.length - 1]; return [last.querySelector(".dice-label").textContent, last.querySelector(".dice-detail").title];`),
      ["Athletics", "1d20 + 6"],
    );
    // A tile rolls a check — what one is, the world's rule check says.
    await page.js(`return [...document.querySelectorAll("#goblin button.ability")][1].click();`);
    await page.settle(200);
    check(
      "a check on a score rolls the rule check on its modifier",
      await page.js(`const e = document.querySelectorAll(".dice-entry"); const last = e[e.length - 1]; return [last.querySelector(".dice-label").textContent, last.querySelector(".dice-detail").title];`),
      ["Dexterity", "1d20 + 2"],
    );

    // 🎲 opens the dice: what was rolled stands in the log; a line rolls what is typed.
    await page.js(`document.querySelector("#dice-button button").click(); return 1;`);
    await page.settle(200);
    check("🎲 opens the side bar with the log", await page.js(`return [...document.querySelectorAll(".dice-panel .dice-entry .dice-label")].map((l) => l.textContent);`), ["Scimitar", "Athletics", "Dexterity"]);
    const typeLine = (text) => page.js(`
      const input = document.querySelector(".dice-input input");
      Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value").set.call(input, ${JSON.stringify(text)});
      input.dispatchEvent(new Event("input", { bubbles: true }));
      return 1;`);
    await typeLine("Attack: 2d6 + 1");
    await page.settle(100);
    await page.js(`document.querySelector(".dice-input input").dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true })); return 1;`);
    await page.settle(200);
    const typed = await page.js(`const e = [...document.querySelectorAll(".dice-panel .dice-entry")].at(-1); return [e.querySelector(".dice-label").textContent, e.querySelector(".dice-detail").title, Number(e.querySelector(".dice-total").textContent)];`);
    check("a typed roll, named before the colon", [typed[0], typed[1], typed[2] >= 3 && typed[2] <= 13], ["Attack", "2d6 + 1", true]);
    await page.clickText(".dice-input button", "☆");
    await page.settle(200);
    check("☆ keeps it as a favourite", await page.js(`return [...document.querySelectorAll(".dice-favorite button:first-child")].map((b) => b.textContent);`), ["Attack"]);
    await page.js(`document.querySelector(".dice-favorite button").click(); return 1;`);
    await page.settle(200);
    check("a favourite rolls with a click", await page.js(`return document.querySelectorAll(".dice-panel .dice-entry").length;`), 5);
    await typeLine("");
    await page.clickText(".dice-pick button", "d20");
    await page.clickText(".dice-pick button", "d20");
    await page.clickText(".dice-pick button", "d6");
    await page.settle(100);
    check("the dice add up in the line", await page.js(`return document.querySelector(".dice-input input").value;`), "2d20 + 1d6");
    await page.clickText(".dice-panel button", "Clear log");
    await page.settle(200);
    check("the GM clears the log", await page.js(`return document.querySelectorAll(".dice-panel .dice-entry").length;`), 0);
    await page.js(`document.querySelector(".dice-panel .side-panel-head button").click(); return 1;`);
    await page.settle(100);

    // While writing, the block's title chooses it: its settings in the side bar.
    await page.js(`[...document.querySelectorAll("#visual-brakka .block-mark .block-title")].find((b) => b.textContent === "Stats").click(); return 1;`);
    await page.settle(200);
    check(
      "a block's title, while writing, opens its settings: the type's, with size, place and folding",
      await page.js(`const p = document.querySelector(".side-panel.inspector"); return p && [p.querySelector("h3").textContent, p.querySelector("h5").textContent, [...p.querySelectorAll(".segmented button.on")].map((b) => b.textContent), p.querySelector(".block-settings select").value];`),
      ["Section · Stats", "Access", ["H1", "Top", "Open"], "statblock"],
    );
    check(
      "who sees and changes the section stands on top — then what the type says",
      await page.js(`return [...document.querySelectorAll(".side-panel.inspector h5")].map((h) => h.textContent);`),
      ["Access", "Type · Character"],
    );
    await page.js(`[...document.querySelector(".side-panel.inspector .segmented").querySelectorAll("button")][2].click(); return 1;`);
    await page.settle(200);
    check("a smaller heading is saved to the type", await page.js(`return window.savedTypes.at(-1).blocks.find((b) => b.key === "stats").heading;`), 3);
    // A ＋ beside the title: the types it makes — all of them to pick from, in the window.
    await page.js(`[...document.querySelectorAll(".side-panel.inspector .search-pick input")].at(-1).focus(); return 1;`);
    await page.settle(150);
    check(
      "the types to pick stand whole in the window, all of them",
      await page.js(`const l = document.querySelector(".search-pick-list"); const r = l.getBoundingClientRect(); return [r.left >= 0 && r.right <= innerWidth && r.top >= 0 && r.bottom <= innerHeight, l.querySelectorAll("li").length === window.stageTypes];`),
      [true, true],
    );
    await page.js(`
      const input = [...document.querySelectorAll(".side-panel.inspector .search-pick input")].at(-1);
      input.focus();
      Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value").set.call(input, "Charac");
      input.dispatchEvent(new Event("input", { bubbles: true }));
      return 1;`);
    await page.settle(100);
    await page.key("Enter");
    await page.settle(300);
    check("a button beside the title makes a type's documents", await page.js(`return window.savedTypes.at(-1).blocks.find((b) => b.key === "stats").new;`), ["Character"]);
    check("one stands beside its title", await page.js(`return [...document.querySelectorAll("#visual-brakka .block-mark")].filter((m) => m.querySelector(".create-button.small")).map((m) => [m.querySelector(".block-title").textContent, m.querySelector(".create-button button").title]);`), [["Notes", "New Character"]]);
    await page.js(`document.querySelector(".side-panel.inspector .side-panel-head button").click(); return 1;`);
    await page.js(`document.querySelector("#visual-brakka .block.at-top .head-fold").click(); return 1;`);
    await page.settle(100);
    check("and the arrow before it folds it", await page.js(`return document.querySelector("#visual-brakka .block.at-top").classList.contains("folded");`), true);
    await page.js(`document.querySelector("#visual-brakka .block.at-top .head-fold").click(); return 1;`);
    await page.settle(100);

    // Moving blocks: in the side bar up and down, among those where it stands.
    const pick = (title) => page.js(`[...document.querySelectorAll("#visual-brakka .block-mark .block-title")].find((b) => b.textContent === ${JSON.stringify(title)}).click(); return 1;`);
    const orderButtons = () => page.js(`const p = document.querySelector(".side-panel.inspector"); return [...p.querySelectorAll(".segmented")][2] && [...[...p.querySelectorAll(".segmented")][2].querySelectorAll("button")].map((b) => [b.textContent.trim(), b.disabled]);`);
    await pick("Story");
    await page.settle(150);
    check("the first in the text moves down, not up", await orderButtons(), [["↑ Up", true], ["↓ Down", false]]);
    const lastSaved = () => page.js(`const t = window.savedTypes.at(-1); return t && [t.id, t.blocks.map((b) => b.key + (b.place && b.place !== "flow" ? ":" + b.place : "")), t.order ?? []];`);
    await page.js(`[...[...document.querySelector(".side-panel.inspector").querySelectorAll(".segmented")][2].querySelectorAll("button")][1].click(); return 1;`);
    await page.settle(200);
    check("down puts it after the next — within its type", await lastSaved(), ["character", ["stats:top", "notes", "story", "family:bottom"], []]);
    await page.js(`document.querySelector(".side-panel.inspector .side-panel-head button").click(); return 1;`);
    // A new section below Story — in the type, for every character.
    await pick("Story");
    await page.settle(150);
    await page.clickText(".side-panel.inspector button", "＋ Section below");
    await page.settle(300);
    check(
      "＋ Section below: a section of the type, right after this one",
      await page.js(`return window.savedTypes.at(-1).blocks.map((b) => b.title);`),
      ["Stats", "Story", "New section", "Notes", "Family"],
    );
    await page.js(`document.querySelector(".side-panel.inspector .side-panel-head button")?.click(); return 1;`);
    // The right-click menu: one entry per element, and the section below.
    await page.js(`const f = [...document.querySelectorAll("#visual-brakka .flow-field")].find((f) => f.textContent.includes("Born under")); f.dispatchEvent(new MouseEvent("contextmenu", { bubbles: true, cancelable: true, clientX: 300, clientY: 300 })); return 1;`);
    await page.settle(150);
    const offered = await page.js(`return [...document.querySelectorAll(".brick-menu button")].map((b) => b.textContent);`);
    check("the menu offers each element once", ["List", "Timeline", "Map", "New section below"].every((n) => offered.includes(n)) && !offered.includes("Cards") && !offered.includes("Timeline of this document"), true);
    await page.key("Escape");
    await page.settle(100);
    // Selected text: a right click formats it.
    await page.js(`
      const f = [...document.querySelectorAll("#visual-brakka .flow-field")].find((f) => f.textContent.includes("Born under"));
      const walker = document.createTreeWalker(f, NodeFilter.SHOW_TEXT);
      let node; while ((node = walker.nextNode()) && !node.textContent.includes("Born")) {}
      const r = document.createRange(); const at = node.textContent.indexOf("Born");
      r.setStart(node, at); r.setEnd(node, at + 4);
      getSelection().removeAllRanges(); getSelection().addRange(r);
      f.dispatchEvent(new MouseEvent("contextmenu", { bubbles: true, cancelable: true, clientX: 300, clientY: 300 }));
      return 1;`);
    await page.settle(150);
    check("selected, a right click offers to format it", await page.js(`return [...document.querySelectorAll(".brick-formats button")].map((b) => b.title).slice(0, 3);`), ["Bold", "Italic", "Code"]);
    await page.js(`document.querySelector(".brick-formats button[title=Bold]").click(); return 1;`);
    await page.settle(300);
    check("and bold it is", await page.js(`return [...document.querySelectorAll("#visual-brakka .flow-bold")].some((b) => b.textContent.replace(/\\*/g, "") === "Born");`), true);
    // On a heading, a right click cuts the text into sections at the headings as high.
    await page.js(`
      const f = [...document.querySelectorAll("#visual-brakka .flow-field")].find((f) => f.textContent.includes("Deep ones"));
      const walker = document.createTreeWalker(f, NodeFilter.SHOW_TEXT);
      let node; while ((node = walker.nextNode()) && !node.textContent.includes("Mines")) {}
      const r = document.createRange(); r.setStart(node, node.textContent.indexOf("Mines") + 1); r.collapse(true);
      getSelection().removeAllRanges(); getSelection().addRange(r);
      f.dispatchEvent(new MouseEvent("contextmenu", { bubbles: true, cancelable: true, clientX: 300, clientY: 300 }));
      return 1;`);
    await page.settle(150);
    await page.js(`[...document.querySelectorAll(".brick-menu button")].find((b) => b.textContent === "## headings to sections").click(); return 1;`);
    await page.settle(300);
    check("on a heading, a right click makes each heading as high a section", await page.js(`return [...document.querySelectorAll("#visual-brakka .block-mark .block-title")].map((b) => b.textContent).slice(-3);`), ["Lore", "Mines", "Wars"]);
    check("each as large as its heading was", await page.js(`return [...document.querySelectorAll("#visual-brakka .block-mark")].filter((m) => /Mines|Wars/.test(m.textContent)).map((m) => [...m.classList].find((c) => c.startsWith("head-")));`), ["head-2", "head-2"]);
    await page.js(`[...document.querySelectorAll("#visual-brakka .block-mark .block-title")].find((b) => b.textContent === "Wars").click(); return 1;`);
    await page.settle(200);
    check("its title opens what a type's section has: size, style, place, folding", await page.js(`return [...document.querySelectorAll(".side-panel.inspector .block-settings > .small.muted")].map((s) => s.textContent);`), ["Title", "Heading", "Style", "Place", "Order", "Folds", "New"]);
    await page.js(`[...document.querySelectorAll(".side-panel.inspector .segmented button")].find((b) => b.textContent === "Bottom").click(); return 1;`);
    await page.settle(300);
    check("placed at the bottom, it stands below the text", await page.js(`return [...document.querySelectorAll("#visual-brakka .block.at-bottom .block-title")].map((b) => b.textContent);`), ["Wars"]);
    await page.js(`document.querySelector(".side-panel.inspector .side-panel-head button").click(); return 1;`);
    check("what stood before the first stays", await page.js(`return [...document.querySelectorAll("#visual-brakka .flow-field")].map((f) => f.textContent.trim()).filter((t) => /Old tales|Deep ones|Many/.test(t));`), ["Old tales.", "Deep ones.", "Many."]);
    // A section of a single part: a click into its text opens no side bar —
    // its access is the section's, set on its title.
    const storyAt = await page.js(`const f = [...document.querySelectorAll("#visual-brakka .flow-field")].find((f) => f.textContent.includes("under the mountain")); f.scrollIntoView({ block: "center" }); const r = f.getBoundingClientRect(); return { x: r.left + 20, y: r.top + 8 };`);
    for (const type of ["mousePressed", "mouseReleased"]) await page.call("Input.dispatchMouseEvent", { type, x: storyAt.x, y: storyAt.y, button: "left", clickCount: 1 });
    await page.settle(300);
    check("a single part has no side bar of its own", await page.js(`return !!document.querySelector(".side-panel.inspector");`), false);
    await page.js(`document.querySelector(".side-panel.inspector .side-panel-head button")?.click(); return 1;`);
    await page.settle(100);
    // Dragged by its title onto the lower half of the sheet on top: it goes
    // there, and on top.
    await page.js(`
      const titles = [...document.querySelectorAll("#visual-brakka .block-mark .block-title")];
      const notes = titles.find((b) => b.textContent === "Notes");
      const sheet = document.querySelector("#visual-brakka .block.at-top");
      const data = new DataTransfer();
      notes.dispatchEvent(new DragEvent("dragstart", { bubbles: true, dataTransfer: data }));
      const box = sheet.getBoundingClientRect();
      const at = { bubbles: true, cancelable: true, dataTransfer: data, clientX: box.left + 20, clientY: box.bottom - 5 };
      sheet.dispatchEvent(new DragEvent("dragover", at));
      window.__drop = () => sheet.dispatchEvent(new DragEvent("drop", at));
      return 1;`);
    await page.settle(100);
    check("while dragged over, the line shows where it lands", await page.js(`return document.querySelector("#visual-brakka .block.at-top").classList.contains("drop-after");`), true);
    await page.js(`window.__drop(); return 1;`);
    await page.settle(300);
    check(
      "dropped: first its place, then its order",
      await page.js(`return window.savedTypes.slice(-2).map((t) => t.blocks.map((b) => b.key + (b.place && b.place !== "flow" ? ":" + b.place : "")));`),
      [["stats:top", "story", "notes:top", "family:bottom"], ["stats:top", "notes:top", "story", "family:bottom"]],
    );

    // Brakka's sheet while writing: its values change where they stand.
    await page.js(`document.querySelector("#visual-brakka .group-view").click(); return 1;`);
    await page.settle(200);
    check(
      "while writing, each skill's proficiency is changed in the list",
      await page.js(`return [...[...document.querySelectorAll("#visual-brakka .group-view .listing")][0].querySelectorAll(".pick-count")].map((i) => i.value);`),
      ["1", "2", "0.5"],
    );
    check(
      "and each saving throw's, under its score",
      await page.js(`return [...document.querySelectorAll("#visual-brakka .ability-also .pick-count")].map((i) => i.value);`),
      ["1", "1", ""],
    );
    await page.js(`
      const input = [...[...document.querySelectorAll("#visual-brakka .group-view .listing")][0].querySelectorAll(".pick-count")][2];
      const set = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value").set;
      set.call(input, "1");
      input.dispatchEvent(new Event("input", { bubbles: true }));
      return 1;`);
    await page.settle(200);
    check(
      "and only numbers other than 0 are written",
      await page.text("#brakka-skills"),
      "@@Skill:Athletics@@ 1, @@Skill:Perception@@ 2, @@Skill:Stealth@@ 1",
    );
    check("each score holds its own value to change", await page.js(`return [...document.querySelectorAll("#visual-brakka .ability-own")].map((i) => i.value);`), ["15", "12", "12", "10", "13", "8"]);
    check(
      "the levels of each class, to change in the sheet",
      await page.js(`return [...document.querySelectorAll("#visual-brakka .group-view .sheet-input .pick-count")].map((i) => i.value);`),
      ["3", "1"],
    );

    // The editor's properties: the group's folded under its name.
    await page.js(`return document.querySelector("#visual .properties > .fold").click();`);
    await page.settle(150);
    const rows = () => page.js(`return document.querySelectorAll("#visual .properties .property-row").length;`);
    check("a group stands folded under its name", await page.js(`return document.querySelector("#visual .property-group > .fold")?.innerText.replace(/\\s+/g, " ").trim();`), "▸ Abilities 6");
    check("folded, only the others show", await rows(), 2);
    await page.js(`return document.querySelector("#visual .property-group > .fold").click();`);
    await page.settle(150);
    check("open, the group's too", await rows(), 8);
  },

  /**
   * Maps: dragged and zoomed like a map on the web, pins that stay where they
   * lie, a stair that leads to the cellar's map in place — and pins set and
   * moved while writing.
   */
  async maps(page) {
    await page.openStage("maps");
    await page.settle(800);
    const layer = (where) => page.js(`return document.querySelector("${where} .map-layer").style.transform;`);
    const scale = async (where) => Number(/scale\(([\d.]+)\)/.exec(await layer(where))[1]);
    check("the picture fits in, its markers on it", await page.js(`return [...document.querySelectorAll("#read .map-pin")].map((p) => p.innerText.trim());`), ["Stair down", "Lighthouse", "Smugglers' cave"]);
    check(
      "a pin set apart stands on its map, marked — and its part shows nothing of its own",
      await page.js(`return [document.querySelector("#read .map-pin.secret")?.innerText.trim(), document.querySelectorAll("#read .block-part").length, document.querySelector("#read").innerText.includes("<pins")];`),
      ["Smugglers' cave", 1, false],
    );
    check("a document waiting on the GM says so", await page.js(`return document.querySelector("#pending .chip.waiting")?.textContent;`), "Waits for the GM");
    // A create button: the GM's, a click starts a new document of its type.
    check("a create button says what it makes", await page.js(`return document.querySelector("#read .create-button button")?.textContent;`), "New place");
    await page.js(`document.querySelector("#read .create-button button").click(); return 1;`);
    await page.settle(100);
    check("and a click starts one", await page.text("#placed"), "new place");
    await page.js(`document.querySelector("#placed-reset").click(); return 1;`);
    check("while writing it is a surface with its settings", await page.js(`return !!document.querySelector('#visual .surface-card[data-label="Create button"] .create-button');`), true);
    check("on a page without a type too", await page.js(`return document.querySelector("#loose .map-pin.secret")?.innerText.trim();`), "Smugglers' cave");
    check("a pin's name stands where it says", await page.js(`return [...document.querySelectorAll("#read .map-pin")].map((p) => [...p.classList].find((c) => c.startsWith("label-")));`), ["label-right", "label-bottom", "label-right"]);
    check("a marker may be a sign — a stair", await page.js(`return document.querySelectorAll("#read .map-pin.icon .marker-icon").length;`), 1);
    const fitted = await scale("#read");
    // The wheel zooms around the pointer.
    const at = await page.spot("#read .map-box");
    await page.call("Input.dispatchMouseEvent", { type: "mouseWheel", x: at.x, y: at.y, deltaX: 0, deltaY: -400 });
    await page.settle(150);
    check("the wheel zooms in", (await scale("#read")) > fitted * 1.5, true);
    // Dragged, the picture follows; the markers stay where they lie on it.
    const before = await layer("#read");
    for (const [type, dx] of [["mousePressed", 0], ["mouseMoved", 40], ["mouseMoved", 120], ["mouseReleased", 120]])
      await page.call("Input.dispatchMouseEvent", { type, x: at.x + dx, y: at.y - 120, button: "left", buttons: type === "mouseReleased" ? 0 : 1, clickCount: 1 });
    await page.settle(150);
    check("dragged, it moves", (await layer("#read")) !== before, true);
    check("a marker keeps its place on the picture", await page.js(`return document.querySelector("#read .map-pin").style.left;`), "30%");
    await page.js(`[...document.querySelectorAll("#read .map-controls button")].find((b) => b.title === "All of it").click(); return 1;`);
    await page.settle(150);
    check("⤢ shows all of it again", Math.abs((await scale("#read")) - fitted) < 0.001, true);
    // A region lights up under the pointer, its name shown.
    const region = await page.js(`const r = document.querySelector("#read .map-area").getBoundingClientRect(); return { x: r.left + r.width / 2, y: r.top + r.height / 2 };`);
    await page.call("Input.dispatchMouseEvent", { type: "mouseMoved", x: region.x, y: region.y });
    await page.settle(150);
    check("a region under the pointer shows its name, above all else", await page.js(`return document.querySelector("#read .map-hover-label")?.innerText.trim();`), "Old town");
    // The stair leads down: the cellar's map, in place.
    await page.click("#read .map-pin.leads .map-pin-badge");
    await page.settle(400);
    check(
      "a marker to a document with a map goes there",
      await page.js(`return [[...document.querySelectorAll("#read .map-pin")].map((p) => p.innerText.trim()), document.querySelector("#read .map-level")?.textContent];`),
      [["Stair up"], "Cellar"],
    );
    await page.clickText("#read .map-controls button", "Back");
    await page.settle(400);
    check("and ‹ comes back", await page.js(`return [...document.querySelectorAll("#read .map-pin")].length;`), 3);
    // A region leads there too.
    for (const type of ["mousePressed", "mouseReleased"]) await page.call("Input.dispatchMouseEvent", { type, x: region.x, y: region.y, button: "left", clickCount: 1 });
    await page.settle(400);
    check("a click on a region goes where it leads", await page.js(`return document.querySelector("#read .map-level")?.textContent;`), "Cellar");
    await page.clickText("#read .map-controls button", "Back");
    await page.settle(300);
    await page.click("#read .map-pin:not(.leads) .map-pin-mark");
    await page.settle(200);
    check("a marker without a document stays", await page.text("#placed"), "");

    // While writing: the side bar lists what is on the map.
    await page.js(`document.querySelector("#visual .map-view").scrollIntoView({ block: "center" }); return 1;`);
    await page.settle(150);
    await page.click("#visual .map-card");
    await page.settle(200);
    check(
      "the side bar lists the markers and regions, not the code",
      await page.js(`return [[...document.querySelectorAll(".inspector .map-list-item")].map((b) => b.innerText.replace(/\\s+/g, " ").trim()), document.querySelectorAll(".inspector pre").length];`),
      [["Stair down → Cellar", "Lighthouse", "Old town → Cellar"], 0],
    );
    // ＋ Marker, a click — a new marker, written; named in the side bar; a door.
    await page.clickText("#visual .map-controls button", "Marker");
    const box = await page.spot("#visual .map-box");
    for (const type of ["mousePressed", "mouseReleased"]) await page.call("Input.dispatchMouseEvent", { type, x: box.x, y: box.y - 90, button: "left", clickCount: 1 });
    await page.settle(300);
    await page.js(`
      const input = document.querySelector(".inspector input[placeholder='Name']");
      const set = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value").set;
      set.call(input, "Well");
      input.dispatchEvent(new Event("input", { bubbles: true }));
      return 1;`);
    await page.settle(200);
    await page.click(".inspector .map-marker-choice[title=door]");
    await page.settle(200);
    check("a click sets a marker, named and a door", /<pin at=[\d.]+,[\d.]+ label=Well icon=door>/.test(await page.text("#map-text")), true);
    // Dragged, a marker moves with the pointer.
    const pin = await page.spot("#visual .map-pin:last-of-type .map-pin-badge");
    for (const [type, dx] of [["mousePressed", 0], ["mouseMoved", 30], ["mouseMoved", 80], ["mouseReleased", 80]])
      await page.call("Input.dispatchMouseEvent", { type, x: pin.x + dx, y: pin.y, button: "left", buttons: type === "mouseReleased" ? 0 : 1, clickCount: 1 });
    await page.settle(300);
    const well = (await page.text("#map-text")).split("\n").find((l) => l.includes("Well")) ?? "";
    check("dragged, a marker moves with the pointer", Number(/at=([\d.]+)/.exec(well)?.[1]) > 58, true);
    // ＋ Region: a click at each corner, the first again to close it.
    await page.clickText("#visual .map-controls button", "Region");
    for (const [dx, dy] of [[180, -150], [260, -150], [260, -100]])
      for (const type of ["mousePressed", "mouseReleased"]) await page.call("Input.dispatchMouseEvent", { type, x: box.x + dx, y: box.y + dy, button: "left", clickCount: 1 });
    await page.settle(200);
    await page.click("#visual .map-corner.closes");
    await page.settle(300);
    check("corner by corner, a region of its own", ((await page.text("#map-text")).match(/<area /g) ?? []).length, 2);
    check("chosen at once, its corners to drag", await page.js(`return document.querySelectorAll("#visual .map-corner:not(.draft):not(.between)").length;`), 3);
    const areas = async () => [...(await page.text("#map-text")).matchAll(/<area at="([^"]*)"/g)].map((m) => m[1].split(" ").length);
    // Between two corners: dragged, a corner more — the border finer.
    await page.js(`document.querySelector("#visual .map-view").scrollIntoView({ block: "center", behavior: "instant" }); return 1;`);
    await page.settle(300);
    const mid = await page.spot("#visual .map-corner.between");
    for (const [type, dy] of [["mousePressed", 0], ["mouseMoved", 10], ["mouseMoved", 30], ["mouseReleased", 30]])
      await page.call("Input.dispatchMouseEvent", { type, x: mid.x, y: mid.y + dy, button: "left", buttons: type === "mouseReleased" ? 0 : 1, clickCount: 1 });
    await page.settle(300);
    check("dragged from between two corners, a corner more", (await areas())[1], 4);
    // A right click on a corner takes it out — down to three.
    await page.js(`document.querySelector("#visual .map-corner:not(.between):not(.draft)").dispatchEvent(new MouseEvent("contextmenu", { bubbles: true, cancelable: true, clientX: 300, clientY: 300 })); return 1;`);
    await page.settle(150);
    await page.clickText(".popup-menu button", "Remove the corner");
    await page.settle(300);
    check("a right click on a corner removes it", (await areas())[1], 3);
    await page.js(`document.querySelector("#visual .map-corner:not(.between):not(.draft)").dispatchEvent(new MouseEvent("contextmenu", { bubbles: true, cancelable: true, clientX: 300, clientY: 300 })); return 1;`);
    await page.settle(150);
    check("three are the least", await page.js(`return [...document.querySelectorAll(".popup-menu button")].find((b) => b.textContent === "Remove the corner")?.disabled;`), true);
    await page.key("Escape");
    // A right click on a region: a corner where it was clicked.
    await page.js(`const p = [...document.querySelectorAll("#visual .map-area")][0]; const r = p.getBoundingClientRect(); p.dispatchEvent(new MouseEvent("contextmenu", { bubbles: true, cancelable: true, clientX: r.left + r.width / 2, clientY: r.top + 2 })); return 1;`);
    await page.settle(150);
    await page.clickText(".popup-menu button", "Add a corner here");
    await page.settle(300);
    check("a right click on a region adds a corner there", (await areas())[0], 5);
    // A marker's name only under the pointer; a marker only the GM sees.
    await page.js(`[...document.querySelectorAll(".inspector .map-list-item")].find((b) => b.textContent.includes("Lighthouse")).click(); return 1;`);
    await page.settle(200);
    await page.clickText(".inspector .map-label-at button", "On hover");
    await page.settle(200);
    check("a name shown only on hover keeps its side", /<pin at=72.5,45 label=Lighthouse label-at="bottom hover">/.test(await page.text("#map-text")), true);
    check(
      "shown while chosen, not otherwise",
      await page.js(`const p = [...document.querySelectorAll("#visual .map-pin")].find((p) => p.textContent === "Lighthouse"); const shown = () => getComputedStyle(p.querySelector(".map-pin-label")).display; const now = shown(); p.classList.remove("chosen"); const then = shown(); p.classList.add("chosen"); return [now, then];`),
      ["block", "none"],
    );
    await page.clickText(".inspector button", "GM only");
    await page.settle(200);
    check(
      "GM only: on the tag, marked in the list and on the map",
      await page.js(`return [/label=Lighthouse label-at="bottom hover" view=gm edit=gm>/.test(document.querySelector("#map-text").textContent), !![...document.querySelectorAll(".inspector .map-list-item")].find((b) => b.textContent.includes("Lighthouse"))?.querySelector(".map-list-secret"), document.querySelectorAll("#visual .map-pin.secret").length];`),
      [true, true, 1],
    );
    await page.clickText(".inspector button", "As the map");
    await page.settle(200);
    check("as the map again: revealed", /label=Lighthouse label-at="bottom hover">/.test(await page.text("#map-text")), true);
    // A right click in the side bar's list takes a marker out.
    await page.js(`[...document.querySelectorAll(".inspector .map-list-item")].find((b) => b.textContent.includes("Lighthouse")).dispatchEvent(new MouseEvent("contextmenu", { bubbles: true, cancelable: true, clientX: 1200, clientY: 400 })); return 1;`);
    await page.settle(150);
    await page.clickText(".popup-menu button", "Remove the marker");
    await page.settle(300);
    check("a right click in the list removes it", /Lighthouse/.test(await page.text("#map-text")), false);
  },

  async relations(page) {
    await page.openStage("relations");
    const names = (where) =>
      page.js(`return [...document.querySelectorAll("${where} .person .name")].map((p) => p.textContent).sort();`);
    const top = (where, name) =>
      page.js(`
        const g = [...document.querySelectorAll("${where} .person")].find((g) => g.querySelector(".name").textContent === "${name}");
        return g.getBoundingClientRect().top;`);
    check("Tim's tree holds his parents and his brother", await names("#tim"), ["Anna", "Franklin", "Simon", "Tim"]);
    check("without a setting no line has an arrowhead", await page.js(`return document.querySelectorAll("#tim path[marker-end], #tim path[marker-start]").length;`), 0);
    check(
      "each role stands at its line",
      [...new Set(await page.js(`return [...document.querySelectorAll("#tim .pill .role")].map((t) => t.textContent);`))].sort(),
      ["Brother", "Father", "Mother", "Son"],
    );
    check(
      "the surface is as high as the tree — nothing cut off, nothing to scroll",
      await page.js(`return [...document.querySelectorAll(".graph-scroll")].every((s) => s.scrollHeight <= s.clientHeight + 1);`),
      true,
    );
    check("everyone shows Ada's unreadable mother as ???", (await names("#web")).includes("???"), true);
    // A right click on free ground, then a name: the person stands there.
    await page.js(`
      const s = document.querySelector("#edit svg.graph");
      const r = s.getBoundingClientRect();
      s.dispatchEvent(new MouseEvent("contextmenu", { bubbles: true, clientX: r.left + 30, clientY: r.top + 30 }));
      return true;`);
    await page.settle(300);
    await page.js(`[...document.querySelectorAll(".graph-menu button")].find((b) => b.textContent === "Ada").click(); return true;`);
    await page.settle(400);
    check("a person added from the graph stands in it", (await names("#edit")).includes("Ada"), true);

    // A click on a person opens their relations in the side bar.
    const marie = await page.js(`
      const g = [...document.querySelectorAll("#edit .person")].find((g) => g.querySelector(".name").textContent === "Marie");
      g.scrollIntoView({ block: "center" });
      const r = g.getBoundingClientRect();
      return { x: r.left + r.width / 2, y: r.top + r.height / 2 };`);
    for (const type of ["mouseMoved", "mousePressed", "mouseReleased"])
      await page.call("Input.dispatchMouseEvent", { type, x: marie.x, y: marie.y, button: "left", buttons: type === "mousePressed" ? 1 : 0, clickCount: 1 });
    await page.settle(300);
    check(
      "a click on Marie opens her card in the side bar",
      await page.js(`return document.querySelector(".side-panel h3")?.textContent;`),
      "Marie",
    );
    await page.js(`document.querySelector('.side-panel .card-settings .swatch-button[title="rose"]').click(); return true;`);
    await page.settle(300);
    check(
      "her frame takes the colour chosen",
      await page.js(`
        const g = [...document.querySelectorAll("#edit .person")].find((g) => g.querySelector(".name").textContent === "Marie");
        return g.classList.contains("rel-rose");`),
      true,
    );

    // The visual editor: a card moved stands where it was put — in the tag.
    const tim = await page.js(`
      const g = [...document.querySelectorAll("#visual .person")].find((g) => g.querySelector(".name").textContent === "Tim");
      g.scrollIntoView({ block: "center" });
      const r = g.querySelector("rect").getBoundingClientRect();
      return { x: r.left + 40, y: r.top + r.height / 2 };`);
    const mouse = (type, p) =>
      page.call("Input.dispatchMouseEvent", { type, x: p.x, y: p.y, button: "left", buttons: type === "mouseReleased" ? 0 : 1, clickCount: 1 });
    await mouse("mouseMoved", tim);
    await mouse("mousePressed", tim);
    for (let i = 1; i <= 6; i++) await mouse("mouseMoved", { x: tim.x + 20 * i, y: tim.y + 10 * i });
    await mouse("mouseReleased", { x: tim.x + 120, y: tim.y + 60 });
    await page.settle(400);
    const code = () => page.js(`return document.querySelector("#tim-code").textContent;`);
    check("a moved card's place stands in the graph", /<person who=Tim at=\d+,\d+>/.test(await code()), true);
    check(
      "the visual side shows the graph, not its code",
      await page.js(`return [...document.querySelectorAll("#visual textarea")].some((t) => t.value.includes("<graph"));`),
      false,
    );

    // A click on a line opens its roles and how it is drawn — only its own.
    await page.js(`document.querySelector(".side-panel .side-panel-head button")?.click(); return true;`);
    await page.settle(200);
    // A click on a line opens its roles and how it is drawn.
    await page.js(`
      const hit = [...document.querySelectorAll("#visual .edge .hit")][0];
      hit.scrollIntoView({ block: "center" });
      const p = hit.getPointAtLength(hit.getTotalLength() / 2).matrixTransform(hit.getScreenCTM());
      window.__hit = { x: p.x, y: p.y };
      return true;`);
    const hit = await page.js(`return window.__hit;`);
    for (const type of ["mouseMoved", "mousePressed", "mouseReleased"])
      await page.call("Input.dispatchMouseEvent", { type, x: hit.x, y: hit.y, button: "left", buttons: type === "mousePressed" ? 1 : 0, clickCount: 1 });
    await page.settle(300);
    check(
      "a line's settings open in the side bar",
      /^\S+ – \S+$/.test(await page.js(`return document.querySelector(".side-panel .relation-card-head strong")?.textContent;`)),
      true,
    );
    await page.js(`document.querySelector('.side-panel .swatch-button[title="amber"]').click(); return true;`);
    await page.settle(300);
    check("a colour chosen stands in the graph", /<line between="[^"]+" color=amber>/.test(await code()), true);

    // Docking: from Marie's dot onto the line between Hartmut and Olga —
    // her two lines become one, from a point on theirs.
    await page.js(`document.querySelector(".side-panel .side-panel-head button")?.click(); return true;`);
    await page.settle(200);
    const spots = await page.js(`
      const svg = document.querySelector("#edit svg.graph");
      svg.scrollIntoView({ block: "center" });
      const marie = [...svg.querySelectorAll(".person")].find((g) => g.querySelector(".name").textContent === "Marie");
      const dot = marie.querySelector(".handle").getBoundingClientRect();
      const couple = [...svg.querySelectorAll(".edge")].find((g) => g.querySelector("title")?.textContent === "Husband – Wife");
      const hit = couple.querySelector(".hit");
      const p = hit.getPointAtLength(hit.getTotalLength() * 0.5).matrixTransform(hit.getScreenCTM());
      return { from: { x: dot.left + dot.width / 2, y: dot.top + dot.height / 2 }, to: { x: p.x, y: p.y } };`);
    await mouse("mouseMoved", spots.from);
    await mouse("mousePressed", spots.from);
    for (let i = 1; i <= 10; i++)
      await mouse("mouseMoved", { x: spots.from.x + ((spots.to.x - spots.from.x) * i) / 10, y: spots.from.y + ((spots.to.y - spots.from.y) * i) / 10 });
    await mouse("mouseReleased", spots.to);
    await page.settle(400);
    check("a line dropped onto a line docks there", await page.js(`return document.querySelectorAll("#edit .join-point").length;`), 1);
    check(
      "and one card opens for both of Marie's relations",
      /^(Marie – \S+ & \S+|\S+ & \S+ – Marie)$/.test(await page.js(`return document.querySelector(".side-panel .relation-card-head strong")?.textContent;`)),
      true,
    );

    check(
      "a line from line to line that is nearly upright is drawn upright",
      await page.js(`
        const d = document.querySelector("#bridge .edge.joined path:not(.hit):not(.stand-in)").getAttribute("d");
        const [x1, , x2] = d.match(/-?[\\d.]+/g).map(Number);
        return Math.abs(x1 - x2) < 0.01;`),
      true,
    );

    check(
      "a label lies over a line docking where it stands",
      await page.js(`
        const svg = document.querySelector("#crossed svg.graph");
        svg.scrollIntoView({ block: "center" });
        const dot = [...svg.querySelectorAll(".join-point")].map((c) => c.getBoundingClientRect()).find((r) => r.top > svg.getBoundingClientRect().top + 150);
        const hit = document.elementFromPoint(dot.left + dot.width / 2, dot.top + dot.height / 2);
        return !!hit.closest(".labels .pill");`),
      true,
    );
    check(
      "an end of several says a word of its own, or each one's role",
      await page.js(`return [...document.querySelectorAll("#bridge .edge.joined .role")].map((r) => r.textContent);`),
      ["Parents", "Father / Son / Brother"],
    );

    // A line onto a line: the parents' line joined to their children's.
    await page.openStage("relations");
    const chooseLine = async (title) => {
      const p = await page.js(`
        const svg = document.querySelector("#edit svg.graph");
        svg.scrollIntoView({ block: "center" });
        const g = [...svg.querySelectorAll(".edge")].find((g) => g.querySelector("title")?.textContent === ${JSON.stringify(title)});
        const hit = g.querySelector(".hit");
        const q = hit.getPointAtLength(hit.getTotalLength() * 0.3).matrixTransform(hit.getScreenCTM());
        return { x: q.x, y: q.y };`);
      for (const type of ["mouseMoved", "mousePressed", "mouseReleased"]) await mouse(type, p);
      await page.settle(300);
    };
    await chooseLine("Husband – Wife");
    const bridge = await page.js(`
      const from = document.querySelector("#edit .line-handle").getBoundingClientRect();
      const g = [...document.querySelectorAll("#edit .edge")].find((g) => g.querySelector("title")?.textContent === "Brother – Sister");
      const hit = g.querySelector(".hit");
      const q = hit.getPointAtLength(hit.getTotalLength() * 0.5).matrixTransform(hit.getScreenCTM());
      return { from: { x: from.left + from.width / 2, y: from.top + from.height / 2 }, to: { x: q.x, y: q.y } };`);
    await mouse("mouseMoved", bridge.from);
    await mouse("mousePressed", bridge.from);
    for (let i = 1; i <= 10; i++)
      await mouse("mouseMoved", { x: bridge.from.x + ((bridge.to.x - bridge.from.x) * i) / 10, y: bridge.from.y + ((bridge.to.y - bridge.from.y) * i) / 10 });
    await mouse("mouseReleased", bridge.to);
    await page.settle(400);
    check("a line dropped onto a line joins both", await page.js(`return document.querySelectorAll("#edit .join-point").length;`), 2);
    check(
      "one choice per person sets their role towards the other end",
      await page.js(`return [...document.querySelectorAll(".side-panel .connector-end > label > span")].map((s) => s.textContent).length;`),
      4,
    );

    const pick = await page.js(`
      const select = document.querySelector(".side-panel .connector-end > label select");
      const role = [...select.options].find((o) => o.value && o.value !== "mixed" && o.value !== select.value).value;
      select.value = role;
      select.dispatchEvent(new Event("change", { bubbles: true }));
      return role;`);
    await page.settle(600);
    check(
      "a role chosen for one person stands towards everyone at the other end",
      await page.js(`return document.querySelector(".side-panel .connector-end > label select").value;`),
      pick,
    );
    check(
      "and nothing unknown is written in its place",
      await page.js(`return [...document.querySelectorAll("#edit .edge.joined .role")].some((r) => r.textContent.includes("???"));`),
      false,
    );

    check(
      "the panel goes by end: each one's people, and its label",
      await page.js(`return [...document.querySelectorAll(".side-panel .connector-end")].map((e) => [e.querySelectorAll(":scope > label").length, !!e.querySelector(".group-label")]);`),
      [[2, true], [2, true]],
    );
    check("own text suggests nothing", await page.js(`return [...document.querySelectorAll(".side-panel input")].some((i) => i.placeholder === "Parents");`), false);

    // A right click on a line offers to remove it; Delete removes a chosen one.
    const before = await page.js(`return document.querySelectorAll("#edit .edge").length;`);
    await page.js(`
      const g = [...document.querySelectorAll("#edit .edge")].find((g) => g.querySelector("title")?.textContent === "Nemesis");
      const r = g.querySelector(".hit").getBoundingClientRect();
      g.dispatchEvent(new MouseEvent("contextmenu", { bubbles: true, clientX: r.left + r.width / 2, clientY: r.top + r.height / 2 }));
      return true;`);
    await page.settle(200);
    check("a right click on a line offers to remove it", await page.js(`return [...document.querySelectorAll(".graph-menu button")].map((b) => b.textContent);`), ["Settings", "Remove"]);
    await page.js(`document.querySelector(".graph-menu").dispatchEvent(new MouseEvent("mousedown", { bubbles: false })); return true;`);
    await page.js(`[...document.querySelectorAll(".graph-menu button")].find((b) => b.textContent === "Remove").click(); return true;`);
    await page.settle(300);
    check("and removes it", await page.js(`return document.querySelectorAll("#edit .edge").length;`), before - 1);
    await chooseLine("Husband – Wife");
    await page.call("Input.dispatchKeyEvent", { type: "keyDown", key: "Delete", code: "Delete", windowsVirtualKeyCode: 46 });
    await page.call("Input.dispatchKeyEvent", { type: "keyUp", key: "Delete", code: "Delete", windowsVirtualKeyCode: 46 });
    await page.settle(300);
    check(
      "Delete removes the line chosen",
      await page.js(`return [...document.querySelectorAll("#edit .edge title")].some((t) => t.textContent === "Husband – Wife");`),
      false,
    );

    // A right click on free ground offers whom to add.
    await page.js(`
      const s = document.querySelector("#visual svg.graph");
      const r = s.getBoundingClientRect();
      s.dispatchEvent(new MouseEvent("contextmenu", { bubbles: true, clientX: r.left + 30, clientY: r.top + 30 }));
      return true;`);
    await page.settle(300);
    check(
      "a right click on the surface offers new and existing people",
      await page.js(`return [...document.querySelectorAll(".graph-menu button")].map((b) => b.textContent).slice(0, 2);`),
      ["＋ New Character", "Ada"],
    );
  },
};

// --- Running --------------------------------------------------------------

const wanted = process.argv.slice(2);
const running = wanted.length ? wanted : Object.keys(SCENARIOS);

const page = await Page.open(BASE);
page.openStage = async (name) => {
  await page.call("Page.navigate", { url: `${BASE}/?stage=${name}` });
  await page.settle(1100);
};

for (const name of running) {
  const scenario = SCENARIOS[name];
  if (!scenario) {
    console.error(`No scenario “${name}”. Possible: ${Object.keys(SCENARIOS).join(", ")}`);
    process.exitCode = 1;
    continue;
  }
  console.log(`\n${name}`);
  try {
    await scenario(page);
  } catch (e) {
    console.log(`  ✗ aborted: ${e.message}`);
    process.exitCode = 1;
  }
}

page.close();
console.log(process.exitCode ? "\nSomething is off." : "\nAll good.");
