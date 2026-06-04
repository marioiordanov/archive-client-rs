//
//  FinderSync.swift
//  ArchiveClientFinderSync
//
//  Created by Mario Yordanov on 19.02.26.
//

import Cocoa
import FinderSync

class FinderSync: FIFinderSync {

    let SHOW_ALL_SUBMENU_THRESHOLD: Int = 3
    let SHOW_ALL_SUBMENU_NEW_WINDOW_THRESHOLD: Int = 10
    let cache = RevisionCache.shared

    // TODO: load myFolderUrl from some config file or ask ArchiveClientRs
    override init() {
        super.init()
        
        print("FinderSync() launched from %@", Bundle.main.bundlePath as NSString)
        let client = ArchiveSocketClient()
        client.getMappedFolder { result in
            DispatchQueue.main.async {
                switch result {
                case .folder(let folder) where !folder.isEmpty:
                    FIFinderSyncController.default().directoryURLs = [URL(fileURLWithPath: folder)]
                default:
                    var token: Int32 = 0
                    print("subscribe to notification")
                    notify_register_dispatch("com.archiveClientRs.mappedFolderChanged", &token, .main) { _ in
                        print("notification received")
                        let client = ArchiveSocketClient()
                        client.getMappedFolder { result in
                            DispatchQueue.main.async {
                                if case .folder(let folder) = result {
                                    FIFinderSyncController.default().directoryURLs = [URL(fileURLWithPath: folder)]
                                    print("notification observer cancelled")
                                    notify_cancel(token)
                                }
                            }
                        }
                    }
                }
            }
        }

        
    }

    override func menu(for menuKind: FIMenuKind) -> NSMenu? {
        guard menuKind == .contextualMenuForItems else { return nil }

        let menu = NSMenu(title: "")

        guard let selected = FIFinderSyncController.default().selectedItemURLs(),
              let fileURL = selected.first else {
            return menu
        }

        guard !fileURL.pathComponents.contains(".archived") else { return nil }
        if fileURL.hasDirectoryPath { return nil }

        print(selected)

        let revisionsItem = NSMenuItem(title: "Archived Versions", action: nil, keyEquivalent: "")
        let submenu = NSMenu(title: "Versions")

        let client = ArchiveSocketClient()
        switch client.getRevisions(for: fileURL.path, force_refresh: false) {
        case .loading:
            let loadingItem = submenu.addItem(withTitle: "Loading...", action: nil, keyEquivalent: "")
            loadingItem.isEnabled = false
        case .error:
            return nil
        case .loaded(let revisions):
            // dropping first, because this is the current version
            let archived = Array(revisions.dropFirst())

            if archived.isEmpty {
                return nil
            }

            cache.set(path: fileURL.path, revisions: archived)
            guard let tagged = cache.get(path: fileURL.path) else { return nil }

            for (tag, rev) in tagged.prefix(SHOW_ALL_SUBMENU_THRESHOLD) {
                let item = NSMenuItem(
                    title: rev.displayTitle,
                    action: #selector(openRevision(_:)),
                    keyEquivalent: ""
                )
                item.target = self
                item.tag = tag
                submenu.addItem(item)
            }

            let separator = NSMenuItem(title: "------------------", action: nil, keyEquivalent: "")
            separator.isEnabled = false
            let showAllItem = NSMenuItem(title: "Show All Revisions", action: #selector(showAllRevisions(_:)), keyEquivalent: "")
            let refreshItem = NSMenuItem(title: "Refresh Revisions", action: #selector(refreshRevisions(_:)), keyEquivalent: "")

            if tagged.count > SHOW_ALL_SUBMENU_NEW_WINDOW_THRESHOLD {
                submenu.addItem(separator)
                submenu.addItem(refreshItem)
                submenu.addItem(showAllItem)
            } else if tagged.count > SHOW_ALL_SUBMENU_THRESHOLD {
                let showAllSubmenu = NSMenu(title: "Show All")
                for (tag, rev) in tagged.dropFirst(SHOW_ALL_SUBMENU_THRESHOLD) {
                    let item = NSMenuItem(
                        title: rev.displayTitle,
                        action: #selector(openRevision(_:)),
                        keyEquivalent: ""
                    )
                    item.target = self
                    item.tag = tag
                    showAllSubmenu.addItem(item)
                }
                showAllItem.submenu = showAllSubmenu

                submenu.addItem(showAllItem)
                submenu.addItem(separator)
                submenu.addItem(refreshItem)
            }
        }

        revisionsItem.submenu = submenu
        menu.addItem(revisionsItem)

        return menu
    }

    @objc func openRevision(_ sender: NSMenuItem) {
        guard let path = FIFinderSyncController.default().selectedItemURLs()?.first?.path else { return }
        guard let file_with_revision = self.cache.getRevision(tag: sender.tag, path: path) else { return }
        let client = ArchiveSocketClient()
        client.downloadFile(file_with_revision: file_with_revision) { _ in }
    }

    @objc func refreshRevisions(_ sender: NSMenuItem) {
        guard let path = FIFinderSyncController.default().selectedItemURLs()?.first?.path else { return }
        let client = ArchiveSocketClient()
        let _ = client.getRevisions(for: path, force_refresh: true)
    }

    @objc func showAllRevisions(_ sender: NSMenuItem) {
        guard let path = FIFinderSyncController.default().selectedItemURLs()?.first?.path else { return }
        let client = ArchiveSocketClient()
        client.showAllRevisions(path: path)
    }
}


final class RevisionCache {
    static let shared = RevisionCache()

    private struct Entry {
        let revisions: [(tag: Int, revision: FileWithRevision)]
    }

    private var tagCounter = 0
    private var store: [String: Entry] = [:]
    private let lock = NSLock()

    func get(path: String) -> [(tag: Int, revision:FileWithRevision)]? {
        lock.lock()
        defer { lock.unlock() }

        guard let entry = store[path] else { return nil }

        return entry.revisions
    }

    func set(path: String, revisions: [FileWithRevision]) {
        lock.lock()
        defer { lock.unlock() }
        let tagged = revisions.map { rev in
            defer { tagCounter += 1 }
            return (tag: tagCounter, revision: rev)
        }

        store[path] = Entry(revisions: tagged)
    }

    func getRevision(tag: Int, path: String) -> FileWithRevision? {
        lock.lock()
        defer { lock.unlock() }
        guard let entry = store[path] else { return nil }
        return entry.revisions.first(where: { $0.tag == tag })?.revision
    }
}
