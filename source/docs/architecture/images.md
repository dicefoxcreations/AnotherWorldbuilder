# Images

As of October 2026. Builds on [world format 3](world-format.md) and the [journal](journal.md). Code: `crates/aw-store/src/media/` (format, metadata, parts, gallery entry), `journal/` (entry `image`, key in the section), `store/images.rs`, `media.rs` (commands), `network.rs` (parts over the connection); interface: `shared/ui/WorldImage.tsx`, `shared/ui/ImagePicker.tsx`, `features/settings/Gallery.tsx`.

## Goals

- Upload images, give them names, rename them, search the gallery and insert them again.
- Visibility as with text: whoever may read a section sees its images; nothing else.
- **Originals stay unchanged** – no shrinking, no re-encoding. Images are also the basis for maps in high resolution.
- Every device gets all images.
- **Nothing is deleted automatically.** The GM filters the gallery for unused images and removes them by hand.

## Three levels

| Level | Example | Changes | Where |
| --- | --- | --- | --- |
| ID | `img:<author>-<ulid>` | never | in the text: `<image src=img:… caption=…>`, in image fields as the value |
| Name | "Map of the Raven Coast" | can be renamed | in the gallery entry, sealed |
| File | `media/…` | never | in the world folder, named after hashes |

The caption in the document is separate from the gallery name and is set per use. So a revealing name ("Aldric unmasked as the traitor") stays in the gallery, while the document only says "A man in the shadows".

## Storage

```text
world-<id>/
  media/<hash>.json        gallery entry: parts of the original and the preview, sealed details
  media/parts/<hash>.bin   encrypted parts, at most 4 MiB each
```

- Every image has its own random key. The original is cut into parts of at most 4 MiB, each encrypted by itself; the gallery entry names the parts in order. Everything is named after the hash of its bytes and cannot change.
- Parts instead of one file: large maps (hundreds of MiB) travel in pieces over the connection and through folders, and an interruption costs only the last piece.
- For the gallery a **preview** (about 256 px) is made in addition, also encrypted, as a small image of its own. The original is not touched by it – the preview only spares the gallery from loading hundreds of originals.
- Allowed: PNG, JPEG, WebP, GIF. No SVG (can contain scripts), no AVIF for now.
- **Metadata is removed** – losslessly, without touching the pixels: EXIF (location, camera, time), XMP, IPTC, comments and text chunks. Colour profile and orientation are kept; the orientation as a minimal EXIF block that contains nothing else.
- Syncing through folders (`media/`) and the connection like everything else: everything to everyone.

## Visibility

**Inheritance:** as everywhere in the document, the innermost wins. An `<image>` without rights of its own has those of its block or area, which has those of the document. An `<image view=… edit=…>` has exactly these – narrower or wider than its block. The outer limit is the document: whoever may not open it sees no image in it. Image fields of the infobox have the rights of their field in the template.

- On inserting, the editor copies the image key into the section – encrypted with the section's key, beside the text. Whoever may read the section can decrypt the image.
- An image in a GM-only part stays hidden from players; the same image in an open part becomes visible to that part's readers from then on. Whoever could read once keeps the image – as with text.
- **Covered with a placeholder:** an image can carry rights of its own and a placeholder:

  ```
  <image src=img:aldric caption="Aldric, the traitor" view=gm edit=gm
         hidden=img:silhouette hidden-caption="A stranger in the shadows">
  ```

  `view`/`edit` make the image a part of its own with these rights. Whoever cannot open it sees `hidden` with `hidden-caption` instead – a silhouette with "?" for people, a map with "?" for places. Without `hidden`, only with `hidden-caption`, a "?" stands there. The placeholder is stored in the next part the others may read, as `<image src=… caption=… for=<part>>`; it is shown only where the part `<part>` is not open. Whoever can read both sees one tag in the code again. **Uncovering** means changing `view`, to `players` for example.
- A placeholder only arises with `hidden` or `hidden-caption`: an image the GM puts down only for herself does not announce itself.
- **Defaults per image:** in the gallery (*Defaults*), the GM and uploaders set with which rights and which placeholder an image is inserted. The editor writes them into the tag on inserting; there they can be changed per use. The defaults are sealed like the name.

## Gallery

The **gallery entry** (`media/<hash>.json`, signed by the uploader or the GM) contains the ID, the parts of original and preview and – sealed – name, type, dimensions, size, upload date and the image key. Sealed for the GM and the uploader. Renaming writes a new entry; the newest counts.

| Who | sees in the gallery | may |
| --- | --- | --- |
| GM | all images | upload, rename, insert anywhere, delete |
| Player | own uploads and images from sections they can read | upload, rename their own, insert only into sections they may edit |

**Filters** (GM only): *used* (with the documents), *only in old versions*, *unused*. The GM can read all sections and so finds every use; which old versions name an image is found out when the gallery opens and remembered in the index.

**Deleting** only by the GM, by hand, through `Deleted` as with versions: entries and parts disappear on all devices and from the folders. The version limit never deletes images. A document that names a deleted image shows a placeholder.

## Operation

- **Gallery:** Options → *Images* (`features/gallery/Gallery.tsx`). On the left the GM's **folder tree**: folders in folders, foldable, with a count; on hover *＋* (subfolder), *✎* (rename), bin (delete – its images then lie in *In no folder*, its subfolders move up one level; no image is lost). An image whose folder no longer exists lies in none. Images and folders are moved by dragging. The middle shows the chosen folder like a file manager: first its folders (a click opens, dragging an image onto one puts it inside), then its own images. *Images* at the top is the top level, *All images* shows all at once. On the right the chosen image: large, **name** (rename), **folder**, dimensions, **use** (a click opens the document), defaults, delete. Uploading puts into the chosen folder. The search searches all images. **Sort** puts every image without a folder where its first document stands in the navigation (`World › Characters › NPCs`, `World › Monsters › Homebrew`), and creates every folder the navigation has, empty ones too such as `World › Items › Rings › Homebrew` (`features/gallery/Sorting.ts`). If several documents use an image, it belongs to the one that carries it as an image property, then to the one whose name shares the most words with the image name: the picture of a place that a session also uses lies with the place. What no document shows goes to *Unused*. What is already sorted stays where it lies. The image picker while writing shows the GM the same folders and starts where the document stands in the navigation. The GM filters by *Used*, *Old versions only*, *Unused*.
- **Folders** are objects with their own ID (`FolderNode`: id, name, parent); an image names its folder by ID. Renaming and moving a folder is a single entry – the tree stands as `Item::Gallery`, sealed for the GM. Players see their own uploads and the images of readable parts, without folders: how the GM sorts, they do not learn.
- **In code:** right click in the editor → *Image* opens the picker; what is inserted is `<image src=… align=right caption="">` – with the image's defaults (see below).
- **Visually:** a part is **one** writing field (`shared/editor/FlowField.tsx`), not a text field per piece. Images and graphs stand *in* this field; an image aligned left or right floats, the text runs past it and continues below it at full width – as in the read view. The image is as large as when reading, with its caption below. Pointing at it shows above it what it changes: alignment, width, removing, *Hide from players* or, for a covered image, *Others see* with a placeholder and its caption; a click on the image swaps it. Without a width of its own an image has its original size, at most as wide as the text; with a width, exactly that. If a floating image leaves no room beside it (less than 120 px), it stands by itself and the text below – in the editor as when reading. A centred image stands in the middle with its caption – always by itself, text cannot run past it on the left and right at once. Left or right, the text runs past beside it; with `wrap=no` (switch *Text beside*) the image stands at the edge and the text below. An empty line directly after an image or a heading the editor shows as a line (a browser would swallow it). Font, line height and width of the field are those of the read view; headings, bold, italics, code, lists and quotes are drawn as when reading; their marks (`##`, `**`, `-`, `>`) appear only while the cursor is in that spot. Links show the name as when reading – a click opens their text `@@Type:Name@@` for changing. Lists (`<list>`) and timelines (`<timeline>`) stand as a surface in the field like images, drawn as when reading; pointing at them allows changing display and tag ([calendar and timeline](calendar-timeline.md)). A single line break is a break when reading too, and an image tag on a line of its own is a block of its own when reading – the text after it a new paragraph. The text stays the truth: the field is built from it, and what is typed is read back into it; an image always stands on a line by itself in it.
- **Image field of the infobox** (portrait, coat of arms): whoever may edit the document clicks the image and picks one – or *No image*.
- **New documents:** already in the form for a new document, an image field takes an image by click, and every text field has 🖼 (upload or from the gallery). So also for players, whose new documents are only created by the GM's device (see [journal](journal.md#players-create-documents)): they see their image in the waiting document at once; the GM's device finds the image key through the gallery entry, which is sealed for the GM too, and puts it into the section on creating. From then on everyone who may read the section sees it.
- **Uploading reveals nothing yet:** at first only the uploader and the GM see an uploaded image. It only becomes visible in a section others read. If *Review changes* is on, a version with the image waits for the GM like any other.

## Inserting

In the editor a button *Image*: **Upload** (enter a name; suggested is the file name without extension) or **From the gallery** (search by name, previews). The document code holds `<image src=img:<id> caption="…" align=right width=300>` – the existing image tag –, an image field of the template (portrait, coat of arms) holds the ID as its value. The visual side shows the image.

The ID starts with an identifier of the uploader (`img:<author>-<ulid>`): this way it can be checked that only they and the GM write an entry for this image – only these two may rename it.

## Open

- Map feature: tiles in several zoom levels from the original, a concept of its own later.
