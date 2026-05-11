//
//  FinderSync.swift
//  ArchiveClientFinderSync
//
//  Created by Mario Yordanov on 19.02.26.
//

import Cocoa
import FinderSync

class FinderSync: FIFinderSync {

    var myFolderURL = URL(fileURLWithPath: "/Users/mario/Projects/archive-client-rs/test-folder")
    let cache = RevisionCache.shared

    // TODO: load myFolderUrl from some config file or ask ArchiveClientRs
    override init() {
        super.init()

        print("FinderSync() launched from %@", Bundle.main.bundlePath as NSString)

        // Set up the directory we are syncing.
        FIFinderSyncController.default().directoryURLs = [self.myFolderURL]
    }

    override func menu(for menuKind: FIMenuKind) -> NSMenu? {
        guard menuKind == .contextualMenuForItems else { return nil }

        let menu = NSMenu(title: "")

        guard let selected = FIFinderSyncController.default().selectedItemURLs(),
              let fileURL = selected.first else {
            return menu
        }

        print(selected)

        let revisionsItem = NSMenuItem(title: "Archived Versions", action: nil, keyEquivalent: "")
        let submenu = NSMenu(title: "Versions")

        if let revisions = cache.get(path: fileURL.path) {
            let archived = revisions.sorted(by: { $0.revision.modifiedTime > $1.revision.modifiedTime }).dropFirst().prefix(3)
            if archived.isEmpty {
                return nil
            }
            for rev in archived {
                let item = NSMenuItem(
                    title: rev.revision.displayTitle,
                    action: #selector(openRevision(_:)),
                    keyEquivalent: ""
                )
                item.target = self
                item.tag = rev.tag
                submenu.addItem(item)
            }

            submenu.addItem(.separator())

            let refresh = NSMenuItem(
                title: "Refresh",
                action: #selector(refreshRevisions(_:)),
                keyEquivalent: ""
            )
            print("refresh path", fileURL.path)
            submenu.addItem(refresh)

        } else {
            let loadingItem = submenu.addItem(
                withTitle: "Loading...",
                action: nil,
                keyEquivalent: ""
            )
            loadingItem.isEnabled = false
            let client = ArchiveSocketClient()

            client.getRevisions(for: fileURL.path) { revisions in
                guard let revisions else {
                    return
                }

                self.cache.set(path: fileURL.path, revisions: revisions)
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
        client.downloadFile(file_with_revision: file_with_revision)
    }

    @objc func refreshRevisions(_ sender: NSMenuItem) {
        guard let path = FIFinderSyncController.default().selectedItemURLs()?.first?.path else { return }
        let client = ArchiveSocketClient()
        client.getRevisions(for: path) { revisions in
            guard let revisions else { return }
            self.cache.set(path: path, revisions: revisions)
        }
    }

}

final class RevisionCache {
    static let shared = RevisionCache()

    private struct Entry {
        let revisions: [(tag: Int, revision: FileWithRevision)]
        let date: Date
    }

    private var tagCounter = 0
    private var store: [String: Entry] = [:]
    private let ttl: TimeInterval = 60
    private let lock = NSLock()

    func get(path: String) -> [(tag: Int, revision:FileWithRevision)]? {
        lock.lock()
        defer { lock.unlock() }

        guard let entry = store[path] else { return nil }
        guard Date().timeIntervalSince(entry.date) < ttl else {
            store.removeValue(forKey: path)
            return nil
        }

        return entry.revisions
    }

    func isFresh(path: String) -> Bool {
        return get(path: path) != nil
    }

    func set(path: String, revisions: [FileWithRevision]) {
        lock.lock()
        defer { lock.unlock() }
        let tagged = revisions.map { rev in
            defer { tagCounter += 1 }
            return (tag: tagCounter, revision: rev)
        }

        store[path] = Entry(revisions: tagged, date: Date())
    }

    func getRevision(tag: Int, path: String) -> FileWithRevision? {
        lock.lock()
        defer { lock.unlock() }
        guard let entry = store[path] else { return nil }
        return entry.revisions.first(where: { $0.tag == tag })?.revision
    }
}
