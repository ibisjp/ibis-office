// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.

/** The version of the agreement between the host and ibisOffice. ibisOffice refuses a connection whose version differs. */
export declare const IBISOFFICE_HOST_PROTOCOL: 1;
export interface IbisOfficeOptions {
    /** The display language. Default 'auto' (the browser's). */
    locale?: 'ja' | 'en' | 'auto';
    /** The colour scheme. Default 'auto' (the OS's). */
    theme?: 'light' | 'dark' | 'auto';
    /** The menu bar, the toolbar, the close button and the like. */
    ui?: {
        menuBar?: boolean;
        toolBar?: boolean;
        closeButton?: boolean;
        downloadButton?: boolean;
        fontSetupPrompt?: boolean;
        startWindow?: boolean;
    };
    /** Narrows the features down. An item left out is true (usable). */
    features?: {
        /** Show 'open from local storage' and 'save to local (download)'. */
        localFiles?: boolean;
        /** Printing. Usable only when the host gives printPdf (the bridge gives a default). */
        print?: boolean;
        /** Export (saved to the host's storage with saveFileAs). false: all off. An array: only those formats (e.g. ['pdf']). */
        export?: boolean | string[];
        /** Opening links to the outside. */
        openLinks?: boolean;
        /** Downloading the extra fonts (the setup). */
        fontDownload?: boolean;
    };
    /** Auto save. 0 or absent: off. On, saveFile is called with trigger 'auto' this many seconds after a change. */
    autoSaveSeconds?: number;
    /** A watermark drawn over the document. Not saved in the document's file. */
    watermark?: {
        text: string;
        opacity?: number;
        includeInPrint?: boolean;
    };
    /** The user who works (the author of comments and tracked changes, the status display). */
    user?: {
        id?: string;
        name: string;
    };
    /** Where 'source code (AGPL)...' of the help goes: the source of a modified version that is served (AGPL section 13). Default: the ibisOffice project's page. */
    sourceUrl?: string;
    /**
     * The files ibisOffice fetches when it needs them (the sounds of the slide transitions: `resource-list.txt`; the fonts: `font-list.txt`). Absent: the public repository's lists. The host may serve the lists and
     * the files itself (a company's own server; ibisWorks).
     */
    resources?: {
        /**
         * The address of `resource-list.txt`: https, or a loopback address (http://127.0.0.1, http://localhost); a relative address is resolved against the page of the loader. The list's own lines
         * may give a file's `url` (and `license_url`) relative too, against the list's address (`sounds/a/a.ogg`, `/office/sounds/a/a.ogg`).
         */
        listUrl?: string;
        /**
         * The address of `font-list.txt` (the fonts セットアップ offers to download, and the font the PDF falls back to): https, or a loopback address (http://127.0.0.1, http://localhost); a
         * relative address is resolved against the page of the loader. The list's own lines give a font file's `url` (and `license_url`), relative too against the list's address
         * (`fonts/a/a.ttf`, `/office/fonts/a/a.ttf`). Absent, or not an address the editor may fetch: the public repository's list. `features.fontDownload: false` still turns the downloads off.
         */
        fontListUrl?: string;
    };
    /** The host's start view: a page that runs ibisOffice with no document (its 'Open...' is the host's file window). The logo at the left end of the menu bar opens it in a new tab (through openUrl). Absent: the logo does nothing. */
    startUrl?: string;
}
export interface HostDocument {
    /** The file name, with its extension. */
    name: string;
    /** The file's contents. */
    bytes: ArrayBuffer;
    /** The version of these contents. ibisOffice does not look inside; it hands it back as the next saveFile / renameFile's baseVersion. */
    version: string;
    /** Open read-only. */
    readOnly?: boolean;
    /** The text of the information bar when read-only (the host decides the language). */
    readOnlyMessage?: string;
    /** Whether the document is tied to the host's file. false (default true): opened as a local document. */
    bound?: boolean;
    /**
     * The id of the folder the file is in. Optional: the file window (open, save as, export) starts in that folder (without it, in the folder used
     * last in this run, else the top). ibisOffice does not look inside; it hands it back as `folderId` of `listFolder`.
     */
    folderId?: string;
}
export type HostErrorReason = 'locked' | 'conflict' | 'readOnly' | 'noPermission' | 'notFound' | 'exists' | 'tooLarge' | 'unauthorized' | 'network' | 'other';
export interface HostFailure {
    ok: false;
    reason: HostErrorReason;
    /** The text shown to the user (the host decides the language). Absent: ibisOffice's own. */
    message?: string;
    /** Lead the user to 'save as'. Absent: true for locked/conflict/readOnly/notFound/noPermission. */
    offerSaveAs?: boolean;
}
export interface HostFolderRef {
    id: string;
    name: string;
}
/**
 * The kind of a root (what `listFolder(null)` returns). ibisOffice uses it for the icon only and keeps the order the host gave; what a kind
 * means is the host's. 'personal': one's own; 'shared': what all share; 'joined': a container of the places one takes part in (it need not
 * be a real folder); 'pinned': favourites; 'other': anything else (an unknown value is read as 'other').
 */
export type HostRootKind = 'personal' | 'shared' | 'joined' | 'pinned' | 'other';
export interface HostFolder {
    id: string;
    name: string;
    /** Whether it has sub folders (the tree's expand mark). */
    hasSubFolders: boolean;
    /** Whether a file can be saved in this folder. */
    canWrite: boolean;
    /** On the roots (the folders of listFolder(null)): their kind. Absent on an ordinary folder. */
    rootKind?: HostRootKind;
}
/** A recently opened file as the host keeps it. */
export interface HostRecentFile {
    /** The file name shown (with its extension: the icon is chosen from it). */
    name: string;
    /** The address of the page that opens the file (what `getFileLocation` gave). */
    url: string;
}
export interface HostFileEntry {
    id: string;
    name: string;
    /** Bytes. */
    size: number;
    /** ISO 8601. */
    updatedAt: string;
    /** The display name of who updated it. */
    updatedBy?: string;
}
export interface HostFolderListing {
    /** The folder seen; null when folderId is null (the top). */
    folder: (HostFolder & {
        path: HostFolderRef[];
    }) | null;
    /** The sub folders of this call's page, when include.folders. */
    folders: HostFolder[];
    /** The files of this call's page, when include.files. */
    files: HostFileEntry[];
    /** When there is more: the value for the next page. ibisOffice does not look inside; it passes it as the next cursor. */
    nextCursor?: string;
    /**
     * A continuation that takes the host long to make (a slow calculation). ibisOffice does not use it by itself: it shows 続きの表示 at the
     * end of the tree's node and of the list, and asks (the same folderId, cursor = this value) only when the user presses it. The host returns
     * what is quick first and leaves the rest behind this value. With nextCursor too, ibisOffice takes nextCursor to its end first.
     */
    deferredCursor?: string;
}
/**
 * The functions of a storage SaaS that ibisOffice uses, as a type (K0041). Whatever the SaaS (its web API, its sign-in, its permissions, how
 * its folders are made, what is slow) is the host adapter's business; ibisOffice sees only this. `IbisOfficeHost` is this and the rest.
 *
 * What an adapter keeps to:
 * - listFolder: folderId null is the list of roots (each with rootKind), a root's children are asked with the root's id. ids and cursors are
 *   opaque strings of up to 255 characters. The order is the host's. path starts with the root the folder belongs to. canWrite is true only
 *   where saving is possible. A page is at most `limit` items. deferredCursor is the slow continuation, nextCursor the quick one.
 * - checkSaveTarget (optional): answers without the paged list. Without it, ibisOffice reads the rest of the pages of the folder and looks
 *   for the same name there.
 * - saveFileAs: the host judges conflicts and holds the locks; a failure is a HostFailure with its reason.
 * The smallest one (a SaaS where a folder is chosen and a file saved) is listFolder and saveFileAs. ibisOffice turns on what is given.
 */
export type HostStorage = Pick<IbisOfficeHost, 'listFolder' | 'checkSaveTarget' | 'openFile' | 'saveFile' | 'saveFileAs' | 'renameFile' | 'getFileLocation' | 'listRecentFiles' | 'addRecentFile'>;
export interface IbisOfficeHost {
    /**
     * One page of the children of a folder (null: the top). Used by the file window (open, save as, export).
     * There can be hundreds of thousands of folders, so ibisOffice never takes them all: the tree's node, when opened, takes only the
     * sub folders of that folder; the right list takes only the folder chosen. Taken in pages with cursor and limit.
     * A host without it gets no file window: the local file picker and the download are used as before.
     */
    listFolder?(req: {
        folderId: string | null;
        purpose: 'open' | 'saveAs' | 'export';
        include: {
            folders: boolean;
            files: boolean;
        };
        /** The extensions (lower case, no dot) of the files listed. Absent: all. */
        extensions?: string[];
        cursor?: string;
        /** The most items of a page (the host may give fewer). ibisOffice passes 200. */
        limit: number;
    }): Promise<HostFolderListing | HostFailure>;
    /**
     * Before save as / export: whether the folder can be written and whether a file of that name is there. Optional: a host whose list is
     * paged can answer without it being read to the end. Without it ibisOffice reads the remaining pages of the folder and compares the names.
     */
    checkSaveTarget?(req: {
        folderId: string;
        name: string;
        purpose: 'saveAs' | 'export';
    }): Promise<{
        ok: true;
        canWrite: boolean;
        existing?: {
            id: string;
            name: string;
        };
    } | HostFailure>;
    /** A file was chosen in the file window. The host opens it (closes the current document, moves the lock, and gives the new document with load). */
    openFile?(req: {
        fileId: string;
    }): Promise<{
        ok: true;
    } | HostFailure>;
    /** Save (Ctrl+S). bytes: the contents with the edits, in the file's own format. */
    saveFile?(req: {
        bytes: ArrayBuffer;
        baseVersion: string;
        trigger: 'user' | 'auto' | 'close';
    }): Promise<{
        ok: true;
        version: string;
    } | HostFailure>;
    /**
     * Save as, export and make a copy: a new file in a folder of the host (overwriteFileId: over that file).
     * kind 'saveAs': on success the document is tied to the new file (replaced by the returned file / version).
     * 'export' and 'copy': the document's tie does not change.
     */
    saveFileAs?(req: {
        kind: 'saveAs' | 'export' | 'copy';
        folderId: string;
        name: string;
        bytes: ArrayBuffer;
        overwriteFileId?: string;
    }): Promise<{
        ok: true;
        file: {
            id: string;
            name: string;
        };
        version: string;
    } | HostFailure>;
    /** Rename (the extension does not change). On success the new name and version. */
    renameFile?(req: {
        name: string;
        baseVersion: string;
    }): Promise<{
        ok: true;
        name: string;
        version: string;
    } | HostFailure>;
    /** 'Get the path': a URL (or a string for the place) the host decides. ibisOffice only copies it to the clipboard. */
    getFileLocation?(): Promise<{
        ok: true;
        url: string;
    } | HostFailure>;
    /**
     * The files opened lately, newest first (ファイル > 最近使ったファイル; ibisOffice shows at most 20). The host keeps them wherever it decides (ibisWorks: the browser's
     * localStorage). A line opens through the host's `openUrl` with the file's `url`, so the host gives the address of the page that opens the file.
     */
    listRecentFiles?(): Promise<{
        ok: true;
        files: HostRecentFile[];
    } | HostFailure>;
    /**
     * A file was opened (or saved under a new name) in the editor: the host puts it first in its list. `url` is what `getFileLocation` returned for it; the same `url` is not listed
     * twice. Called only when the host also gave `getFileLocation`.
     */
    addRecentFile?(req: HostRecentFile): Promise<{
        ok: true;
    } | HostFailure>;
    /**
     * Open an external URL (the help's 'source code (AGPL)', a link in a document). The host opens a new tab.
     * The check of the scheme and `noopener,noreferrer` are the host's, in one place; ibisOffice does not open it itself.
     * A host without it: opening links is off.
     */
    openUrl?(req: {
        url: string;
    }): Promise<{
        ok: true;
    } | HostFailure>;
    /**
     * Print. ibisOffice makes the PDF and hands it over; the host opens it in a new tab (the browser's PDF viewer prints it).
     * tab: the empty tab the bridge opened right after the click on print (so that the pop-up blocker does not stop it while the PDF is made).
     * The host shows the PDF there, or (tab null) opens a tab itself. A host without it: the bridge's default is used.
     */
    printPdf?(req: {
        name: string;
        bytes: ArrayBuffer;
        tab: WindowProxy | null;
    }): Promise<{
        ok: true;
    } | HostFailure>;
}
export type IbisOfficeEvent = {
    type: 'ready';
} | {
    type: 'loaded';
    name: string;
    kind: 'docx' | 'xlsx' | 'pptx';
} | {
    type: 'dirty';
    dirty: boolean;
} | {
    type: 'documentChanged';
    source: 'local' | 'host' | 'new';
} | {
    type: 'saved';
    trigger: 'user' | 'auto' | 'close';
} | {
    type: 'closeRequested';
} | {
    type: 'download';
    name: string;
    bytes: ArrayBuffer;
} | {
    type: 'error';
    message: string;
};
export interface IbisOfficeHandle {
    /** Open a document (replacing the current one). */
    load(doc: HostDocument): Promise<void>;
    /** Switch read-only on or off. message: the text of the information bar. */
    setReadOnly(readOnly: boolean, message?: string): void;
    /** A notice from the host to the user (the information bar). It replaces the previous one of the same level; null clears. */
    notify(notice: {
        level: 'info' | 'warn' | 'error';
        message: string;
        id?: string;
    } | null): void;
    /** The current contents (the same bytes as a save). For a host that wants to check before closing. */
    snapshot(): Promise<{
        name: string;
        bytes: ArrayBuffer;
    }>;
    /** Clean up (call before removing the iframe). */
    destroy(): void;
}
export interface IbisOfficeMount {
    options?: IbisOfficeOptions;
    host?: IbisOfficeHost;
    onEvent?(event: IbisOfficeEvent): void;
}
/** The functions the bridge hands ibisOffice for printing (same origin form; the cross origin form uses the messages of the same names). */
export interface IbisOfficeBridgeFunctions {
    printBegin(): void;
    printCancel(): void;
}
/** What the loader inside the iframe publishes as `window.IbisOffice` (same origin form). */
export interface IbisOfficeLoader {
    protocol: number;
    /** Resolves when the loader accepts connect (it does not wait for the wasm download). */
    ready: Promise<void>;
    /** Accepts the host's functions once. */
    connect(init: IbisOfficeMount & {
        bridge?: IbisOfficeBridgeFunctions;
    }): Promise<IbisOfficeHandle>;
    /** Direct embedding (the loader and the host in the same page). */
    mount?(container: HTMLElement, init: IbisOfficeMount): Promise<IbisOfficeHandle>;
}
declare global {
    interface Window {
        IbisOffice?: IbisOfficeLoader;
    }
}
/** Direct embedding: the loader (main.js) is in this very page. The editor fills the page; `container` is the element it is for. */
export declare function mount(container: HTMLElement, init: IbisOfficeMount): Promise<IbisOfficeHandle>;
/** The names of the functions of `IbisOfficeHost`. */
export declare const HOST_METHODS: readonly ['listFolder', 'checkSaveTarget', 'openFile', 'saveFile', 'saveFileAs', 'renameFile', 'getFileLocation', 'openUrl', 'printPdf', 'listRecentFiles', 'addRecentFile'];
export type HostMethod = (typeof HOST_METHODS)[number];
export type AttachIframeInit = IbisOfficeMount & {
    /**
     * The origin the iframe is served from. Absent, or the host's own: the same origin form (direct calls of functions).
     * Another origin: the postMessage form, used for targetOrigin and for checking what is received (e.g. 'https://ibisoffice.example.com').
     */
    officeOrigin?: string;
    /** Seconds to wait for the loader (same origin form) or for hello (cross origin form). Default 10. */
    connectTimeoutSeconds?: number;
};
/** Why `attachIframe` could not connect. */
export type ConnectErrorReason = 'originMismatch' | 'loaderTimeout' | 'helloTimeout' | 'connectTimeout' | 'protocolMismatch' | 'refused';
/** What `attachIframe` rejects with when it cannot connect: tell the cause by `reason`, not by the message. */
export declare class IbisOfficeConnectError extends Error {
    readonly reason: ConnectErrorReason;
    constructor(reason: ConnectErrorReason, message: string);
}
/**
 * Connect an iframe that serves ibisOffice. Same origin as the host (or no officeOrigin): the host's functions are handed over as
 * they are and nothing is posted. Another origin: one MessageChannel port after hello / connect.
 */
export declare function attachIframe(iframe: HTMLIFrameElement, init: AttachIframeInit): Promise<IbisOfficeHandle>;
