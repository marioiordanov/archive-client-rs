import Foundation
import Darwin

struct FileWithRevision: Codable {
    let id: String
    let fileId: String
    let modifiedTime: String
    let size: String
    let originalFilename: String

    var displayTitle: String { "\(originalFilename) (\(modifiedTime))" }
}

enum RevisionsResult {
    case loaded([FileWithRevision])
    case loading
    case error
}

class ArchiveSocketClient {
    private let host = "127.0.0.1"
    private let port = 8787

    func getRevisions(for path: String, timeout: TimeInterval = 0.3) -> RevisionsResult {
        let semaphore = DispatchSemaphore(value: 0)
        var result: RevisionsResult = .loading

        DispatchQueue.global().async {
            guard let data = self.sendReceive("revisions@@\(path)") else {
                result = .error
                semaphore.signal()
                return
            }
            if let text = String(data: data, encoding: .ascii) {
                if text == "loading" { semaphore.signal(); return }
                if text == "error" { result = .error; semaphore.signal(); return }
            }
            if let revisions = try? JSONDecoder().decode([FileWithRevision].self, from: data) {
                result = .loaded(revisions)
            } else {
                result = .error
            }
            semaphore.signal()
        }

        _ = semaphore.wait(timeout: .now() + timeout)
        return result
    }
    
    func downloadFile(file_with_revision: FileWithRevision, completion: @escaping (String?) -> Void) {
        DispatchQueue.global().async {
            guard let data = self.sendReceive("download@@\(file_with_revision.fileId)@@\(file_with_revision.id)@@\(file_with_revision.modifiedTime)") else {
                completion(nil)
                return
            }
            completion(String(data: data, encoding: .utf8))
        }
    }

    private func sendReceive(_ message: String) -> Data? {
        let sock = socket(AF_INET, SOCK_STREAM, 0)
        guard sock >= 0 else {
            print("socket() failed: \(String(cString: strerror(errno)))")
            return nil
        }
        defer { Darwin.close(sock) }

        var addr = sockaddr_in()
        addr.sin_family = sa_family_t(AF_INET)
        addr.sin_port = in_port_t(port).bigEndian
        inet_pton(AF_INET, host, &addr.sin_addr)

        let connected = withUnsafePointer(to: &addr) {
            $0.withMemoryRebound(to: sockaddr.self, capacity: 1) {
                Darwin.connect(sock, $0, socklen_t(MemoryLayout<sockaddr_in>.size))
            }
        }
        guard connected == 0 else {
            print("connect() failed: \(String(cString: strerror(errno)))")
            return nil
        }

        var lenLE = UInt16(message.utf8.count).littleEndian
        let packet = withUnsafeBytes(of: &lenLE) { Array($0) } + Array(message.utf8)
        let sent = Darwin.send(sock, packet, packet.count, 0)
        guard sent == packet.count else {
            print("send() failed: \(String(cString: strerror(errno)))")
            return nil
        }

        var result = Data()
        var buf = [UInt8](repeating: 0, count: 4096)
        while true {
            let n = Darwin.recv(sock, &buf, buf.count, 0)
            if n < 0 {
                print("recv() failed: \(String(cString: strerror(errno)))")
                break
            }
            if n == 0 { break }
            result.append(contentsOf: buf.prefix(n))
        }
        return result.isEmpty ? nil : result
    }
}
