"""Building an AnotherWorldbuilder package (`transfer.rs`) from a script.

Ids are derived from what a thing is (a URL, a file path), so running a
converter again yields the same documents — a second import adds only what
is new. Types are written in the template language and read by the app's own
reader through the test bench (`cargo run -p aw-testbench`); property keys
are then fixed the same way.
"""

import hashlib
import json
import os
import re
import shutil
import urllib.request

CROCKFORD = "0123456789ABCDEFGHJKMNPQRSTVWXYZ"
TESTBENCH = "http://127.0.0.1:5320"


def ulid_of(*parts):
    """A ULID-shaped id from a hash — the same for the same input."""
    digest = hashlib.blake2b("\0".join(parts).encode(), digest_size=16).digest()
    n = int.from_bytes(digest, "big")
    out = []
    for _ in range(26):
        out.append(CROCKFORD[n & 31])
        n >>= 5
    return "".join(reversed(out))


def article_id(*parts):
    return "art:" + ulid_of("article", *parts)


def link(target_id, text):
    """A link as stored: `@@art:…|text@@`."""
    text = text.replace("@@", "").replace("|", "/").strip()
    return f"@@{target_id}|{text}@@" if text else f"@@{target_id}@@"


def bench(cmd, args):
    req = urllib.request.Request(TESTBENCH, data=json.dumps({"cmd": cmd, "args": args}).encode(), headers={"content-type": "application/json"})
    answer = json.loads(urllib.request.urlopen(req).read())
    if "error" in answer:
        raise RuntimeError(answer["error"])
    return answer["value"]


class Types:
    """Types written as text, read by the app; keys fixed by type and label."""

    def __init__(self):
        self.order = []
        self.by_id = {}

    def directory(self):
        return [{"id": t["id"], "name": t["name"]} for t in self.by_id.values()]

    def add(self, type_id, name, icon="", text=None, supertype="", group=False, page_text=None, description=""):
        """Declare first (for the directory), read later in `compile`."""
        self.order.append(type_id)
        self.by_id[type_id] = {"id": type_id, "name": name, "icon": icon, "supertype": supertype, "group": group, "description": description, "_text": text}

    def compile(self):
        out = []
        compiled = {}
        for type_id in self.order:
            t = self.by_id[type_id]
            base = {k: v for k, v in t.items() if not k.startswith("_")}
            text = t["_text"] or "<article view=all edit=gm>\n  <name>\n  <body>\n  </body>\n</article>\n"
            read = bench("template_read", {"text": text, "base": base, "directory": self.directory(), "others": out})
            errors = read.get("errors") or []
            if errors:
                raise RuntimeError(f"{t['name']}: {errors}")
            template = read["template"]
            # Fixed keys instead of fresh ones: by type and label.
            renames = {}
            for field in template["properties"]:
                fixed = ulid_of("property", type_id, field["label"]).lower()
                renames[field["key"]] = fixed
                field["key"] = fixed
            for i, block in enumerate(template["blocks"]):
                fixed = ulid_of("block", type_id, block["title"] or str(i)).lower()
                for part in block["parts"]:
                    if part["key"] == block["key"]:
                        part["key"] = fixed
                    else:
                        part["key"] = ulid_of("part", type_id, block["title"] or str(i), part["key"]).lower()
                block["key"] = fixed
            for shown in template["infobox"]["show"]:
                shown["property"] = renames.get(shown["property"], shown["property"])
            template["sort"] = [renames.get(k, k) for k in template.get("sort", [])]
            compiled[type_id] = template
            out.append(template)
        return out

    def effective_fields(self, compiled, type_id):
        """A type's fields with its supertypes' and groups'."""
        out = []
        seen = set()
        todo = [type_id]
        while todo:
            tid = todo.pop()
            if tid in seen or tid not in compiled:
                continue
            seen.add(tid)
            t = compiled[tid]
            for f in t["properties"]:
                out.append(f)
                if f.get("kind") == "group":
                    todo.append(f["group"])
            if t.get("supertype"):
                todo.append(t["supertype"])
        return out


def stored_names(text, keys):
    """`%%Armor class%%` and `bind=Armor class` with keys instead of names."""
    def prop(m):
        name = m.group(1)
        head, _, rest = name.partition(".")
        key = keys.get(head.strip().lower())
        return f"%%{key}{'.' + rest if rest else ''}%%" if key else m.group(0)
    text = re.sub(r"%%([^%]+)%%", prop, text)

    def bind(m):
        name = m.group(2)
        key = keys.get(name.strip().strip('"').lower())
        return f'{m.group(1)}{key}' if key else m.group(0)
    return re.sub(r'(bind=)("[^"]+"|[^\s>]+)', bind, text)


class Package:
    def __init__(self, name):
        self.name = name
        self.written = False
        self.types = []
        self.documents = {}
        self.images = {}
        self.navigation = None
        self.calendar = None
        self.home = None

    def document(self, doc_id, type_id, name, parts, view="all", edit="gm", fields=None, slug=None):
        self.documents[doc_id] = {
            "id": doc_id,
            "type": type_id,
            "slug": slug or re.sub(r"[^a-z0-9]+", "-", name.lower()).strip("-"),
            "name": name,
            "view": view,
            "edit": edit,
            "fields": fields or {},
            "parts": parts,
        }

    def image(self, source_path, name):
        """An image by its file; returns its id (`img:…`) in the package."""
        digest = hashlib.blake2b(open(source_path, "rb").read(), digest_size=16).hexdigest()
        image_id = f"img:{digest[:12]}-{ulid_of('image', digest)}"
        if image_id not in self.images:
            self.images[image_id] = {"id": image_id, "name": name, "source": source_path}
        return image_id

    def write(self, folder):
        os.makedirs(os.path.join(folder, "images"), exist_ok=True)
        images = []
        for image in self.images.values():
            ext = os.path.splitext(image["source"])[1].lower() or ".bin"
            file = f"images/{image['id'][4:]}{ext}"
            target = os.path.join(folder, file)
            if not os.path.exists(target):
                shutil.copyfile(image["source"], target)
            images.append({"id": image["id"], "name": image["name"], "file": file})
        package = {"format": "aw-package", "version": 1, "name": self.name, "written": self.written, "types": self.types, "documents": list(self.documents.values()), "images": images, "navigation": self.navigation}
        if self.calendar:
            package["calendar"] = self.calendar
        if self.home:
            package["home"] = self.home
        with open(os.path.join(folder, "package.json"), "w", encoding="utf-8") as f:
            json.dump(package, f, ensure_ascii=False)


def part(section, text="", view="all", edit="gm", fields=None, title=""):
    return {"section": section, "title": title, "text": text, "fields": fields or {}, "view": view, "edit": edit}


def entry(id_, kind, target="", label="", children=None, gm=False):
    """An entry of the outline: a type, a document or a group."""
    e = {"id": id_, "kind": kind}
    if target:
        e["target"] = target
    if label:
        e["label"] = label
    if gm:
        e["gm"] = True
    if children:
        e["children"] = children
    return e
