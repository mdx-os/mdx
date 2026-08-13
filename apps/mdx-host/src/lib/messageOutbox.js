// The offline outbox: every send goes through here, online or not - one
// path, so exactly-once is a property of the design rather than a special
// case. Each message carries a client-minted message_id (the idempotency
// key the governed write already accepts and the projection echoes back).
// The flush probe makes retries safe: before resending anything that MIGHT
// have landed, ask the projection whether that message_id exists - if the
// kernel has it, the message is saved and the queue entry retires without
// a duplicate. Queue persistence is IndexedDB, so a closed tab or a dead
// battery never loses words someone already wrote.
//
// v1 shipped this same shape (apps/message offline-queue + websocket
// stores); this is that pattern rebuilt on the governed write path.

const DB_NAME = "mdx-message-outbox";
const STORE = "queue";
const BACKOFF_MS = [1000, 2000, 4000, 8000, 16000, 30000];

export function mintMessageId() {
  const uuid = globalThis.crypto?.randomUUID?.() ?? `${Date.now()}-${Math.random().toString(16).slice(2)}`;
  return `msg_${uuid.replace(/-/g, "").slice(0, 20)}`;
}

function openDb() {
  return new Promise((resolve, reject) => {
    const request = indexedDB.open(DB_NAME, 1);
    request.onupgradeneeded = () => {
      if (!request.result.objectStoreNames.contains(STORE)) {
        request.result.createObjectStore(STORE, { keyPath: "messageId" });
      }
    };
    request.onsuccess = () => resolve(request.result);
    request.onerror = () => reject(request.error);
  });
}

async function dbAll(db) {
  return new Promise((resolve, reject) => {
    const request = db.transaction(STORE, "readonly").objectStore(STORE).getAll();
    request.onsuccess = () => resolve(request.result ?? []);
    request.onerror = () => reject(request.error);
  });
}

async function dbPut(db, item) {
  return new Promise((resolve, reject) => {
    const request = db.transaction(STORE, "readwrite").objectStore(STORE).put(item);
    request.onsuccess = () => resolve();
    request.onerror = () => reject(request.error);
  });
}

async function dbDelete(db, messageId) {
  return new Promise((resolve, reject) => {
    const request = db.transaction(STORE, "readwrite").objectStore(STORE).delete(messageId);
    request.onsuccess = () => resolve();
    request.onerror = () => reject(request.error);
  });
}

// send(item) performs the governed write and resolves when the kernel
// confirmed it. probe(messageId) answers whether the projection already
// carries the id (the retry-safety check). onChange(items) hands the
// current queue to the surface after every transition.
export function createOutbox({ send, probe, onChange, onRefused }) {
  let db = null;
  let items = [];
  let flushing = false;
  let retryTimer = null;
  let attempts = 0;

  const notify = () => onChange?.([...items]);

  async function load() {
    try {
      db = await openDb();
      items = (await dbAll(db)).sort((a, b) => a.queuedAtMs - b.queuedAtMs);
      // Anything caught mid-send by a closed tab might have landed; the
      // probe decides on the next flush, not a guess now.
      for (const item of items) {
        if (item.status === "sending") item.status = "queued";
      }
      notify();
    } catch (error) {
      // No IndexedDB (private mode): the queue lives in memory only and
      // says nothing false - sends still work, persistence is best-effort.
      db = null;
    }
    return items.length;
  }

  async function persist(item) {
    if (db) {
      try {
        await dbPut(db, item);
      } catch (error) {
        // memory-only fallback
      }
    }
  }

  async function retire(messageId) {
    items = items.filter((item) => item.messageId !== messageId);
    if (db) {
      try {
        await dbDelete(db, messageId);
      } catch (error) {
        // already gone is fine
      }
    }
    notify();
  }

  async function enqueue({ messageId, channelId, threadId, body }) {
    const item = {
      messageId,
      channelId,
      threadId: threadId ?? "",
      body,
      queuedAtMs: Date.now(),
      status: "queued",
      attempted: false,
      lastError: ""
    };
    items = [...items, item];
    await persist(item);
    notify();
    flush();
    return item;
  }

  async function flush() {
    if (flushing) return;
    flushing = true;
    try {
      for (const item of [...items]) {
        if (item.status === "sending") continue;
        // Retry safety: if a previous attempt might have reached the
        // kernel, ask before resending. The projection's message_id echo
        // is the truth; a hit means saved, not "send again".
        if (item.attempted) {
          try {
            if (await probe(item.messageId)) {
              await retire(item.messageId);
              continue;
            }
          } catch (error) {
            // The probe needs the kernel; if it is unreachable the send
            // below fails the same way and the item stays queued.
          }
        }
        item.status = "sending";
        item.attempted = true;
        await persist(item);
        notify();
        try {
          await send(item);
          attempts = 0;
          await retire(item.messageId);
        } catch (error) {
          if (error?.terminal) {
            // A policy refusal is an answer, not an outage: retire the
            // item (retrying would refuse forever) and tell the person.
            await retire(item.messageId);
            onRefused?.(item, error.message);
            continue;
          }
          item.status = "failed";
          item.lastError = "not sent yet";
          await persist(item);
          notify();
          scheduleRetry();
          break;
        }
      }
    } finally {
      flushing = false;
    }
  }

  function scheduleRetry() {
    clearTimeout(retryTimer);
    const base = BACKOFF_MS[Math.min(attempts, BACKOFF_MS.length - 1)];
    attempts += 1;
    // Jitter keeps a thousand reconnecting clients from stampeding.
    const delay = base * (0.75 + Math.random() * 0.5);
    retryTimer = setTimeout(() => {
      for (const item of items) {
        if (item.status === "failed") item.status = "queued";
      }
      notify();
      flush();
    }, delay);
  }

  function retryNow() {
    attempts = 0;
    clearTimeout(retryTimer);
    for (const item of items) {
      if (item.status === "failed") item.status = "queued";
    }
    notify();
    flush();
  }

  function dispose() {
    clearTimeout(retryTimer);
  }

  return { load, enqueue, flush, retryNow, dispose, get items() { return items; } };
}
