import { useContext, useMemo, useRef, useState, type RefObject } from "react";
import { Icon } from "../../shared/ui/Icons";
import { type ArticleView, type BlockPart, type GraphNode, type WorldInfo } from "../../api/client";
import { linkIcon } from "./DocLink";
import { RegionAccess } from "../../shared/ui/Visibility";
import { TableCard } from "./TableCard";
import { linker, linkIn, MARK } from "../../shared/Links";
import { anchorRange } from "../../shared/Anchors";
import { RemoteCarets, type RemoteCaret } from "../../shared/editor/RemoteCarets";
import { type MarkKind, type Range } from "../../shared/Marks";
import { sideBarOwner, useSideBar } from "../../shared/ui/SideBar";
import { AccessSection, Inspector, InspectorSection, PartAccessContext, PieceMoveContext } from "./Inspector";
import { articleById } from "../../shared/types";
import { mapText, pieces as graphPieces } from "../../shared/GraphText";
import { CurrentDocument } from "../worlds/WorldData";
import { ImageCard, ImageRow, imageSurface } from "./ImageCard";
import { AbilitiesCard, CreateCard, EmbedCard, ListCard, TimeCard, ViewCard } from "./ListCard";
import { MapCard } from "./MapView";
import { attr, writeImageLine } from "../../shared/ImageTag";
import { FlowField, FlowHandle, FlowMarks } from "../../shared/editor/FlowField";
import type { Piece } from "../../shared/GraphText";
import { HeadingInspector, headingAt, LinkInspector, type SectionActions, SectionInspector } from "./EditorInspectors";
import { type Links, SectionBelow, useBricks } from "./EditorTools";

/**
 * One part of a section: its text, with what stands in it. Who may see and
 * change it is set in the side bar — it opens for the game master as soon
 * as the caret is in the part, unless something else holds the bar.
 */
export function PartEditor({
  world,
  part,
  title,
  inherited,
  removable,
  links,
  carets,
  marks,
  onPart,
  onRemove,
  onCaret,
  graph,
  region,
  docAccess,
  sections,
  editor = false,
}: {
  world: WorldInfo;
  part: BlockPart;
  /** Not the GM, but may edit it: sets who sees it. */
  editor?: boolean;
  /** The section's title — to name the part by. */
  title: string;
  inherited: boolean;
  /** A region with access of its own: says whom it is for at its top. */
  region?: boolean;
  /** The document's access — what a region's differs from. */
  docAccess: { view: string; edit: string };
  /** What is done to a section chosen by its anchor — across the document. */
  sections: SectionActions;
  removable: boolean;
  links: Links;
  carets: RemoteCaret[];
  marks: { kind: MarkKind; ranges: Range[] };
  onPart: (p: Partial<BlockPart>) => void;
  onRemove: () => void;
  onCaret: (start: number, end: number) => void;
  /** Draws a graph standing in the text — the `i`-th piece of it. */
  graph: (g: GraphNode, i: number) => React.ReactNode;
}) {
  // Framed where it is a region of its own; the first part's access stands
  // at the section's title — framed too, it would read as a section inside one.
  const guarded = !!region;
  // The part is one field: its text, with the images and graphs standing in
  // it — as surfaces, where the text runs past them as it does when read.
  // Cut once per text — not again when only someone's caret moved.
  const shown = useMemo(() => graphPieces(part.text), [part.text]);
  const images = shown.filter((p) => p.kind === "image");
  // A part that holds nothing but one image, with access of its own: the
  // others see its stand-in.
  const ownPart = !inherited && images.length === 1 && shown.every((p) => p.kind === "image" || !p.text.trim());
  /** The text with the `i`-th piece written anew — or taken out. */
  const withPiece = (i: number, text: string | null) =>
    shown.map((p, j) => (j !== i ? p.text : text === null ? "" : text + (p.text.endsWith("\n") ? "\n" : ""))).join("");
  // Paragraphs and elements in order — what an element moves past.
  const units = useMemo(
    () =>
      shown.flatMap((p, j) =>
        p.kind === "text"
          ? p.text.split(/\n[ \t]*\n/).map((t) => ({ text: t.replace(/^\n+|\n+$/g, ""), piece: -1 })).filter((u) => u.text.trim())
          : [{ text: p.text.replace(/^\n+|\n+$/g, ""), piece: j }],
      ),
    [shown],
  );
  /** Element `i` past the paragraph or element above (-1) or below (1). */
  const moves = (i: number) => {
    const at = units.findIndex((u) => u.piece === i);
    const by = (step: -1 | 1) => {
      const to = at + step;
      if (at < 0 || to < 0 || to >= units.length) return undefined;
      return () => {
        const next = [...units];
        [next[at], next[to]] = [next[to], next[at]];
        onPart({ text: next.map((u) => u.text).join("\n\n") + "\n" });
      };
    };
    return { up: by(-1), down: by(1) };
  };
  const flow = useRef(new FlowHandle());
  // Nothing but one element in it: the element's access is the part's.
  const surfaces = shown.filter((p) => p.kind !== "text");
  const alone = surfaces.length === 1 && shown.every((p) => p.kind !== "text" || !p.text.trim());
  const [inspecting, setInspecting] = useState(false);
  // A section chosen by a click on its anchor: its name in the side bar, its place marked.
  const [chosenSection, setChosenSection] = useState<string | null>(null);
  useSideBar(`anchor:${part.key}`, !!chosenSection, () => setChosenSection(null));
  const sectionPlace = chosenSection ? anchorRange(part.text, chosenSection) : null;
  // A link chosen by a click: where it leads and what it says, in the side bar.
  const [chosenLink, setChosenLink] = useState<{ start: number; raw: string } | null>(null);
  useSideBar(`link:${part.key}`, !!chosenLink, () => setChosenLink(null));
  const linkPlace = chosenLink && part.text.slice(chosenLink.start, chosenLink.start + chosenLink.raw.length) === chosenLink.raw ? chosenLink : null;
  // The heading the caret stands in, by where its line starts: its level in the side bar.
  const [headingLine, setHeadingLine] = useState<number | null>(null);
  const heading = headingLine === null ? null : headingAt(part.text, headingLine);
  useSideBar(`heading:${part.key}`, !!heading, () => setHeadingLine(null));
  /** The heading's marks as asked for — none: plain text. */
  const setLevel = (level: number) => {
    if (headingLine === null) return;
    const rest = part.text.slice(headingLine).replace(/^#{1,6} /, "");
    onPart({ text: part.text.slice(0, headingLine) + (level ? `${"#".repeat(level)} ` : "") + rest });
    if (!level) setHeadingLine(null);
  };
  // As in the right-click menu: a new section below this one.
  const sectionBelow = useContext(SectionBelow);
  const [dismissed, setDismissed] = useState(false);
  useSideBar(`part:${part.key}`, inspecting, () => {
    setInspecting(false);
    setDismissed(false);
  });
  // The caret in the part opens its access — taking the bar from anything but
  // the versions, which one keeps open on purpose.
  const caretIn = (start: number, end: number) => {
    onCaret(start, end);
    // In a heading: its level in the side bar, not the part's access.
    const lineStart = part.text.lastIndexOf("\n", start - 1) + 1;
    if (start === end && headingAt(part.text, lineStart)) {
      setHeadingLine(lineStart);
      return;
    }
    setHeadingLine(null);
    const holder = sideBarOwner();
    // Only a section of several parts has a part's own access to set; one
    // of a single part sets it on the section.
    if (((world.isGm && removable) || editor) && !inspecting && !dismissed && !chosenSection && !holder?.startsWith("versions:")) setInspecting(true);
  };
  return (
    <PartAccessContext.Provider value={{ world, part, title, alone, onPart, onRemove: removable ? onRemove : undefined, editor }}>
    <div
      className={`block-part${guarded ? " guarded" : ""}${inspecting ? " inspected" : ""}`}
    >
      {region && (
        <RegionAccess world={world} view={part.view} edit={part.edit} docView={docAccess.view} docEdit={docAccess.edit} onClick={world.isGm || editor ? () => (setDismissed(false), setInspecting(true)) : undefined} />
      )}
      {inspecting && (
        <Inspector title={title ? `Part · ${title}` : "Part"} onClose={() => (setInspecting(false), setDismissed(true))}>
          <AccessSection />
          {sectionBelow && (
            <InspectorSection title="Section">
              <button className="compact" onClick={sectionBelow}>
                <Icon name="add" /> Section below
              </button>
            </InspectorSection>
          )}
          {removable && (
            <button className="danger compact inspector-remove" onClick={onRemove}>
              Remove this part
            </button>
          )}
        </Inspector>
      )}
      {linkPlace && (
        <LinkInspector
          key={linkPlace.start}
          raw={linkPlace.raw}
          links={links}
          onChange={(raw) => {
            const at = linkPlace.start;
            onPart({ text: part.text.slice(0, at) + raw + part.text.slice(at + linkPlace.raw.length) });
            setChosenLink(raw.startsWith(MARK) ? { start: at, raw } : null);
          }}
          onClose={() => setChosenLink(null)}
        />
      )}
      {heading && (
        <HeadingInspector level={heading.level} words={heading.words} onLevel={setLevel} onClose={() => setHeadingLine(null)} />
      )}
      {chosenSection && (
        <SectionInspector
          key={chosenSection}
          anchor={chosenSection}
          actions={sections}
          onRenamed={setChosenSection}
          onClose={() => setChosenSection(null)}
        />
      )}
      <PartFlow
        world={world}
        text={part.text}
        flow={flow}
        links={links}
        carets={carets}
        marks={marks}
        onAnchor={(name) => {
          setInspecting(false);
          setChosenLink(null);
          setChosenSection(name);
        }}
        onLink={(link) => {
          setInspecting(false);
          setChosenSection(null);
          setChosenLink(link);
        }}
        section={sectionPlace ?? (linkPlace && { start: linkPlace.start, end: linkPlace.start + linkPlace.raw.length })}
        surface={(piece, i) => (
          <PieceMoveContext.Provider value={moves(i)}>
          {piece.kind === "graph" ? (
            <div className="part-graph">{graph(piece.graph, i)}</div>
          ) : piece.kind === "list" ? (
            <ListCard list={piece.list} onChange={(next) => onPart({ text: withPiece(i, next) })} />
          ) : piece.kind === "timeline" ? (
            <TimeCard tag="timeline" attrs={piece.timeline} onChange={(next) => onPart({ text: withPiece(i, next) })} />
          ) : piece.kind === "calendar" ? (
            <TimeCard tag="calendar" attrs={piece.calendar} onChange={(next) => onPart({ text: withPiece(i, next) })} />
          ) : piece.kind === "create" ? (
            <CreateCard attrs={piece.create} onChange={(next) => onPart({ text: withPiece(i, next) })} />
          ) : piece.kind === "abilities" ? (
            <AbilitiesCard attrs={piece.abilities} onChange={(next) => onPart({ text: withPiece(i, next) })} />
          ) : piece.kind === "view" ? (
            <ViewCard attrs={piece.view} onChange={(next) => onPart({ text: withPiece(i, next) })} />
          ) : piece.kind === "embed" ? (
            <EmbedCard attrs={piece.embed} onChange={(next) => onPart({ text: withPiece(i, next) })} />
          ) : piece.kind === "table" ? (
            <TableCard text={piece.text} onChange={(next) => onPart({ text: withPiece(i, next) })} />
          ) : piece.kind === "map" ? (
            <MapCard world={world} map={piece.map} onChange={(next) => onPart({ text: withPiece(i, next ? mapText(next) : null) })} />
          ) : piece.kind === "image" ? (
            <ImageCard
              world={world}
              image={piece.image}
              ownPart={ownPart}
              onChange={(next) => onPart({ text: withPiece(i, next ? writeImageLine(next) : null) })}
            />
          ) : piece.kind === "images" ? (
            <ImageRow
              items={piece.images.map(({ image }, j) => ({
                align: attr(image, "align"),
                node: (
                  <ImageCard
                    key={j}
                    world={world}
                    image={image}
                    ownPart={false}
                    onChange={(next) => {
                      // That image's line written anew, or taken out; the others as they are.
                      const lines = piece.images.map((r, k) => (k !== j ? r.text : next ? writeImageLine(next) + (r.text.endsWith("\n") ? "\n" : "") : ""));
                      onPart({ text: withPiece(i, lines.join("").replace(/\n$/, "")) });
                    }}
                  />
                ),
              }))}
            />
          ) : null}
          </PieceMoveContext.Provider>
        )}
        onText={(text) => onPart({ text })}
        onCaret={caretIn}
      />
    </div>
    </PartAccessContext.Provider>
  );
}

/**
 * The text of a part as one flow field — with the right-click bricks, the
 * `@@` suggestions, the marks of a comparison and the carets of others.
 */
function PartFlow({
  world,
  text,
  flow,
  links,
  carets,
  marks,
  surface,
  onText,
  onCaret,
  onAnchor,
  onLink,
  section,
}: {
  world: WorldInfo;
  text: string;
  flow: RefObject<FlowHandle>;
  links: Links;
  carets: RemoteCaret[];
  marks: { kind: MarkKind; ranges: Range[] };
  surface: (piece: Piece, i: number) => React.ReactNode;
  onText: (text: string) => void;
  onCaret: (start: number, end: number) => void;
  /** A click on an anchor — see `FlowField`. */
  onAnchor?: (name: string) => void;
  /** A click on a link — see `FlowField`. */
  onLink?: (link: { start: number; raw: string }) => void;
  /** The chosen section's place in the text, marked. */
  section?: { start: number; end: number } | null;
}) {
  const bricks = useBricks(world.isGm, text, onText, links, flow);
  const { ref: _unused, ...handlers } = bricks.props;
  void _unused;
  // A link as it is read: the name it names — or its own text — and grey where it leads nowhere.
  const resolve = useMemo(() => linker(links.all, links.templates), [links.all, links.templates]);
  const current = useContext(CurrentDocument);
  const linkLook = (written: string) => {
    const link = linkIn(written.slice(MARK.length, -MARK.length));
    if (!link) return { text: written, dead: true };
    // `#^2f9e17`: a line of the target — of this document without a name.
    const anchor = link.property.startsWith("^") ? link.property.slice(1) : undefined;
    const target = anchor && !link.name ? articleById(links.all, current) : resolve(link);
    return {
      text: link.text !== link.name ? link.text : (target?.name ?? link.name),
      dead: !target,
      id: target?.id,
      anchor,
      icon: target && linkIcon(links.templates, target.articleType),
    };
  };
  return (
    <>
      <FlowField
        handle={flow}
        text={text}
        className="block-text"
        placeholder="Write here. @@ links a document, right-click inserts lists, images …"
        surface={surface}
        linkLook={linkLook}
        onAnchor={onAnchor}
        onLink={onLink}
        surfaceClass={(p) =>
          p.kind === "image"
            ? imageSurface(p.image)
            : p.kind
        }
        onText={onText}
        onCaret={onCaret}
        handlers={handlers}
      >
        <FlowMarks handle={flow} text={text} ranges={marks.ranges} kind={marks.kind} />
        {section && <FlowMarks handle={flow} text={text} ranges={[section]} kind="section" />}
        <RemoteCarets field={flow} carets={carets} />
      </FlowField>
      {bricks.element}
    </>
  );
}

/** Whether this reader may edit a part of the document — then, not being the GM, they say who sees it. */
export function mayEditPart(article: ArticleView, key: string): boolean {
  return article.layers.some((l) => l.section === key && l.mayEdit);
}
