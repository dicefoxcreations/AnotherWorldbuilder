import { api, type ArticleView } from "./client";

/**
 * A world as this window reads it, a part at a time — however large it is.
 *
 * First the index: every document without its text, enough for the outline,
 * links, lists and graphs — a part at a time, each after the last; what
 * changed meanwhile is asked for once it is read, never read anew. Then
 * the texts, a part at a time in the background; what someone opens
 * (`want`) is asked for at once, beside it.
 * What others change comes on asking (`sync`): those documents alone — or,
 * where the types or one's own roles changed, the index anew.
 *
 * `onChange` is told the documents after each step: those read whole, the
 * others as the index has them (`partial`) — and whether texts are still to
 * come.
 */
export class WorldReader {
  private order: string[] = [];
  private byId = new Map<string, ArticleView>();
  /** The state read last — what `sync` asks from. */
  private epoch = 0;
  private writes = 0;
  private reader = "";
  /** Texts still to read, in order. */
  private queue: string[] = [];
  /** Changed by others, told by id only: read again first — new ones too. */
  private renew: string[] = [];
  /** Asked for now, outside the order. */
  private asking = new Set<string>();
  private running = false;
  /** Raised by every index read anew: what was asked before it counts no more. */
  private generation = 0;
  /** When a document was last set by `sync` — a text asked for before then is older. */
  private touched = new Map<string, number>();
  private tick = 0;
  /** The first index: `sync` waits for it. */
  private ready: Promise<void> = Promise.resolve();
  private syncing: Promise<void> | null = null;
  private again = false;
  private stopped = false;
  private later: ReturnType<typeof setTimeout> | null = null;

  private readonly world: string;
  private readonly previewAs: string[] | null;
  private readonly onChange: (articles: ArticleView[], loading: boolean) => void;
  private readonly onError: (e: unknown) => void;

  constructor(world: string, previewAs: string[] | null, onChange: (articles: ArticleView[], loading: boolean) => void, onError: (e: unknown) => void) {
    this.world = world;
    this.previewAs = previewAs;
    this.onChange = onChange;
    this.onError = onError;
  }

  /** Reads the index and `first` whole — the page one lands on — then goes on in the background. */
  start(first: string[] = []): Promise<void> {
    this.ready = (async () => {
      await this.readIndex();
      const wanted = first.filter((id) => this.byId.get(id)?.partial);
      if (wanted.length > 0) await this.readTexts(wanted);
      // What changed while the index was read.
      await this.readSince();
    })();
    return this.ready;
  }

  /** Stops: nothing more is asked, nothing told. */
  stop() {
    this.stopped = true;
    if (this.later) clearTimeout(this.later);
  }

  /** Documents someone needs whole now: asked for at once, beside the background. */
  want(...ids: string[]) {
    const missing = ids.filter((id) => this.byId.get(id)?.partial && !this.asking.has(id));
    if (missing.length === 0 || this.stopped) return;
    for (const id of missing) this.asking.add(id);
    this.readTexts(missing)
      .catch(this.onError)
      .finally(() => missing.forEach((id) => this.asking.delete(id)));
  }

  /** What changed since the last reading — one asking at a time; asked again meanwhile, once more after. */
  sync(): Promise<void> {
    if (this.syncing) {
      this.again = true;
      return this.syncing;
    }
    this.syncing = (async () => {
      try {
        await this.ready;
        do {
          this.again = false;
          await this.readSince();
        } while (this.again && !this.stopped);
      } finally {
        this.syncing = null;
      }
    })();
    return this.syncing;
  }

  private async readIndex() {
    const generation = ++this.generation;
    const order: string[] = [];
    const byId = new Map<string, ArticleView>();
    // From where the first part began: what changes later comes with `readSince`.
    const first = await api.viewIndex(this.world, this.previewAs, "");
    for (let part = first; ; ) {
      for (const a of part.articles) {
        if (!byId.has(a.id)) order.push(a.id);
        byId.set(a.id, a);
      }
      if (part.next === null) break;
      part = await api.viewIndex(this.world, this.previewAs, part.next);
    }
    if (this.stopped || generation !== this.generation) return;
    this.order = order;
    this.byId = byId;
    this.epoch = first.epoch;
    this.writes = first.writes;
    this.reader = first.reader;
    this.queue = order.filter((id) => byId.get(id)?.partial);
    this.renew = [];
    this.touched.clear();
  }

  private async readSince() {
    if (this.stopped) return;
    const part = await api.viewSince(this.world, this.previewAs, this.epoch, this.writes);
    if (this.stopped) return;
    if (part.whole || part.epoch !== this.epoch || part.reader !== this.reader) {
      await this.readIndex();
    } else {
      this.tick++;
      for (const a of part.articles) {
        if (!this.byId.has(a.id)) this.order.push(a.id);
        this.byId.set(a.id, a);
        this.touched.set(a.id, this.tick);
      }
      this.forget(part.gone);
      const renewing = new Set(this.renew);
      this.renew.push(...part.changed.filter((id) => !renewing.has(id)));
      this.writes = part.writes;
    }
    this.tell();
    void this.background();
  }

  private forget(gone: string[]) {
    if (gone.length === 0) return;
    const out = new Set(gone);
    for (const id of out) this.byId.delete(id);
    this.order = this.order.filter((id) => !out.has(id));
    this.queue = this.queue.filter((id) => !out.has(id));
  }

  /** The texts, a part at a time, until all are read — once at a time. */
  private async background() {
    if (this.running || this.stopped) return;
    this.running = true;
    try {
      while (!this.stopped) {
        const renewing = this.renew.length > 0;
        const ids = renewing ? this.renew.splice(0, 400) : this.next(400);
        if (ids.length === 0) break;
        await this.readTexts(ids, true, renewing);
      }
    } catch (e) {
      // Asked again with the next change or page.
      this.onError(e);
    } finally {
      this.running = false;
      this.tell();
    }
  }

  /** The next ones still partial, in their order. */
  private next(count: number): string[] {
    const out: string[] = [];
    while (out.length < count && this.queue.length > 0) {
      const id = this.queue.shift()!;
      if (this.byId.get(id)?.partial) out.push(id);
    }
    return out;
  }

  /** Reads whole documents — those changed by others (`renewing`) may be new here. */
  private async readTexts(ids: string[], quietly = false, renewing = false) {
    const generation = this.generation;
    const asked = this.tick;
    const part = await api.viewTexts(this.world, this.previewAs, ids);
    if (this.stopped || generation !== this.generation) return;
    for (const a of part.articles) {
      // Set by `sync` after this was asked: that one is newer.
      if ((this.touched.get(a.id) ?? 0) > asked) continue;
      if (!this.byId.has(a.id)) {
        if (!renewing) continue;
        this.order.push(a.id);
      }
      this.byId.set(a.id, a);
    }
    this.forget(part.gone);
    // What did not fit comes next.
    if (renewing) this.renew.unshift(...part.rest);
    else this.queue.unshift(...part.rest);
    this.tell(quietly);
  }

  /**
   * Tells the documents as they are. The background tells at most every
   * moment: each telling draws everything that lists documents anew.
   */
  private tell(quietly = false) {
    if (this.stopped) return;
    const send = () => {
      this.later = null;
      const loading = this.running || this.renew.length > 0 || this.queue.some((id) => this.byId.get(id)?.partial);
      this.onChange(this.order.map((id) => this.byId.get(id)).filter((a): a is ArticleView => !!a), loading);
    };
    if (!quietly) {
      if (this.later) clearTimeout(this.later);
      send();
    } else if (!this.later) this.later = setTimeout(send, 600);
  }
}
