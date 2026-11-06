import { createContext, useContext } from "react";
import type { ArticleView, Place, Template, WorldInfo } from "../../api/client";

/**
 * What the open world holds — for pieces of text that list documents.
 *
 * A `<list>` can stand in any block of any document. Threading the world
 * through every component down to the Markdown would touch everything in
 * between; a context reaches it directly. Outside a world (the template
 * preview, the test bench) there is none, and a list only says what it
 * would show.
 */
export type WorldData = {
  world: WorldInfo;
  templates: Template[];
  /** Only what this reader could decrypt. */
  articles: ArticleView[];
  onPlace: (o: Place) => void;
  /** Loads the documents again — after a change made from a graph. */
  onChanged: () => void;
  /** Tells what went wrong. */
  onError?: (e: unknown) => void;
  /** Opens a place in a window of its own — at a part, by its title. */
  onApart: (o: Place, part?: string) => void;
  /** Takes the types as saved — after one was changed from a document. */
  onTemplates?: (all: Template[]) => void;
  /** Starts a new document of this type — where the app can (`<create>`). */
  onNew?: (articleType: string) => void;
  /** Changes a document this player waits on — in the form it was asked with. */
  onEditPending?: (article: ArticleView) => void;
  /** The game master looks as a player would: reading only, nothing more. */
  previewing?: boolean;
};

export const WorldDataContext = createContext<WorldData | null>(null);

export const useWorldData = () => useContext(WorldDataContext);

/**
 * The parts of the document being drawn that this reader can open, by key.
 * A stand-in image (`<image … for=part>`) shows only where its part does not
 * — otherwise the real image stands there. Outside a document: none.
 */
export const ReadableParts = createContext<Set<string> | null>(null);

/**
 * The document whose text is being drawn — what `around=self` means.
 * Outside one (a template preview) there is none.
 */
export const CurrentDocument = createContext<string | null>(null);
