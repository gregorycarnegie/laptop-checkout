// Browser-storage shim for the Rust app. It never touches SQL: SQLite runs
// inside the Rust/WebAssembly bundle (rusqlite). This file only moves the
// database's bytes to and from the user's PC:
//   * IndexedDB on this computer (always, as an autosave), and
//   * a real .sqlite file the user picks (Chrome/Edge File System Access API).

let fileHandle = null;
let fileWritable = false;

const IDB_NAME = "laptop-checkout";
const IDB_STORE = "kv";
const PICKER_TYPES = [
  {
    description: "SQLite database",
    accept: { "application/vnd.sqlite3": [".sqlite", ".sqlite3", ".db"] },
  },
];

function openIdb() {
  return new Promise((resolve, reject) => {
    const req = indexedDB.open(IDB_NAME, 1);
    req.onupgradeneeded = () => req.result.createObjectStore(IDB_STORE);
    req.onsuccess = () => resolve(req.result);
    req.onerror = () => reject(req.error);
  });
}

async function idb(mode, fn) {
  const conn = await openIdb();
  return new Promise((resolve, reject) => {
    const tx = conn.transaction(IDB_STORE, mode);
    const req = fn(tx.objectStore(IDB_STORE));
    tx.oncomplete = () => resolve(req.result);
    tx.onerror = () => reject(tx.error);
    tx.onabort = () => reject(tx.error);
  });
}

async function idbGet(key) {
  try {
    return await idb("readonly", (s) => s.get(key));
  } catch (_) {
    return undefined;
  }
}
const idbSet = (key, value) => idb("readwrite", (s) => s.put(value, key));
const idbDel = (key) => idb("readwrite", (s) => s.delete(key));

async function readHandle(handle) {
  const file = await handle.getFile();
  return new Uint8Array(await file.arrayBuffer());
}

/** {fs_supported, file_name, file_connected, file_needs_permission} as JSON. */
export function fileInfoJson() {
  return JSON.stringify({
    fs_supported: typeof window.showSaveFilePicker === "function",
    file_name: fileHandle ? fileHandle.name : null,
    file_connected: !!fileHandle && fileWritable,
    file_needs_permission: !!fileHandle && !fileWritable,
  });
}

/** The saved database: the linked file if it is still allowed, else the IndexedDB copy, else null. */
export async function loadInitial() {
  const handle = await idbGet("handle");
  if (handle) {
    fileHandle = handle;
    try {
      fileWritable = (await handle.queryPermission({ mode: "readwrite" })) === "granted";
      if (fileWritable) return await readHandle(handle);
    } catch (_) {
      fileWritable = false;
    }
  }
  const saved = await idbGet("db");
  return saved ? new Uint8Array(saved) : null;
}

/** Saves the database bytes to IndexedDB, and to the linked file when there is one. */
export async function persist(bytes) {
  await idbSet("db", bytes);
  if (fileHandle && fileWritable) {
    const out = await fileHandle.createWritable();
    await out.write(bytes);
    await out.close();
  }
}

/** Asks where to create a new .sqlite file, then writes `bytes` into it. */
export async function createFile(bytes) {
  const handle = await window.showSaveFilePicker({
    suggestedName: "laptop-checkout.sqlite",
    types: PICKER_TYPES,
  });
  fileHandle = handle;
  fileWritable = true;
  await idbSet("handle", handle);
  await persist(bytes);
}

/** Lets the user pick an existing .sqlite file and returns its bytes. */
export async function openFile() {
  const [handle] = await window.showOpenFilePicker({ types: PICKER_TYPES, multiple: false });
  fileWritable = (await handle.requestPermission({ mode: "readwrite" })) === "granted";
  fileHandle = handle;
  await idbSet("handle", handle);
  return await readHandle(handle);
}

/** Browsers ask again for file access after a restart; this must run from a click. */
export async function reconnectFile() {
  if (!fileHandle) return null;
  fileWritable = (await fileHandle.requestPermission({ mode: "readwrite" })) === "granted";
  return fileWritable ? await readHandle(fileHandle) : null;
}

export async function disconnectFile() {
  fileHandle = null;
  fileWritable = false;
  await idbDel("handle");
}

export async function readUpload(file) {
  return new Uint8Array(await file.arrayBuffer());
}

export function downloadBytes(bytes, name) {
  const blob = new Blob([bytes], { type: "application/vnd.sqlite3" });
  const a = document.createElement("a");
  a.href = URL.createObjectURL(blob);
  a.download = name;
  document.body.appendChild(a);
  a.click();
  a.remove();
  setTimeout(() => URL.revokeObjectURL(a.href), 2000);
}

/** Calls `flush` when the tab is hidden, and warns before closing with unsaved changes. */
export function installUnloadGuard(flush, hasUnsaved) {
  document.addEventListener("visibilitychange", () => {
    if (document.visibilityState === "hidden") flush();
  });
  window.addEventListener("beforeunload", (ev) => {
    if (hasUnsaved()) {
      flush();
      ev.preventDefault();
    }
  });
}

/** Copies text, falling back to a hidden textarea where the Clipboard API is refused. */
export async function copyText(text) {
  try {
    await navigator.clipboard.writeText(text);
    return true;
  } catch (_) {
    const ta = document.createElement("textarea");
    ta.value = text;
    ta.setAttribute("readonly", "");
    ta.style.position = "fixed";
    ta.style.opacity = "0";
    document.body.appendChild(ta);
    ta.select();
    let ok = false;
    try {
      ok = document.execCommand("copy");
    } catch (_) {}
    ta.remove();
    return ok;
  }
}
