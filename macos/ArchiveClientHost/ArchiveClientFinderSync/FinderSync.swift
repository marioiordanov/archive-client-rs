//
//  FinderSync.swift
//  ArchiveClientFinderSync
//
//  Created by Mario Yordanov on 19.02.26.
//

import Cocoa
import FinderSync

class FinderSync: FIFinderSync {

    var myFolderURL = URL(fileURLWithPath: "/Users/mario/kibrit-data")

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

        guard !fileURL.pathComponents.contains(".archived") else { return nil }

        print(selected)

        let revisionsItem = NSMenuItem(title: "Archived Versions", action: nil, keyEquivalent: "")
        let submenu = NSMenu(title: "Versions")

        let client = ArchiveSocketClient()
        switch client.getRevisions(for: fileURL.path) {
        case .loading:
            let loadingItem = submenu.addItem(withTitle: "Loading...", action: nil, keyEquivalent: "")
            loadingItem.isEnabled = false
        case .error:
            return nil
        case .loaded(let revisions):
            // dropping first, because this is the current version
            let archived = Array(revisions.reversed().dropFirst())
            if archived.isEmpty {
                return nil
            }

            for rev in archived.prefix(3) {
                let item = NSMenuItem(
                    title: rev.displayTitle,
                    action: #selector(openRevision(_:)),
                    keyEquivalent: ""
                )
                item.target = self
                item.representedObject = rev
                submenu.addItem(item)
            }

            let showAllItem = NSMenuItem(title: "Show All", action: nil, keyEquivalent: "")

            if archived.count > 10 {
                submenu.addItem(showAllItem)
            } else if archived.count > 3 {
                let showAllSubmenu = NSMenu(title: "Show All")
                for rev in archived.dropFirst(3) {
                    let item = NSMenuItem(
                        title: rev.displayTitle,
                        action: #selector(openRevision(_:)),
                        keyEquivalent: ""
                    )
                    item.target = self
                    item.representedObject = rev
                    showAllSubmenu.addItem(item)
                }
                showAllItem.submenu = showAllSubmenu
                submenu.addItem(showAllItem)
            }
        }

        revisionsItem.submenu = submenu
        menu.addItem(revisionsItem)

        return menu
    }

    @objc func openRevision(_ sender: NSMenuItem) {
        guard let rev = sender.representedObject as? FileWithRevision else { return }
        let client = ArchiveSocketClient()
        client.downloadFile(file_with_revision: rev) { _ in }
    }
}
