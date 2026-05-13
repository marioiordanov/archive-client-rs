import Foundation

struct FileWithRevision: Codable {
    let id: String
    let fileId: String
    let modifiedTime: String
    let size: String
    let originalFilename: String

    var displayTitle: String { "\(originalFilename) (\(modifiedTime))" }
}

class ArchiveSocketClient {
    private let host = "127.0.0.1"
    private let port = 8787

    func getRevisions(for path: String, completion: @escaping ([FileWithRevision]?) -> Void) {
        DispatchQueue.global().async {
            guard let data = self.sendReceive("revisions@@\(path)") else {
                completion(nil)
                return
            }
            completion(try? JSONDecoder().decode([FileWithRevision].self, from: data))
        }
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
        var input: InputStream?
        var output: OutputStream?
        Stream.getStreamsToHost(withName: host, port: port, inputStream: &input, outputStream: &output)

        guard let input, let output else { return nil }
        input.open()
        output.open()
        defer { input.close(); output.close() }

        let bytes = Array(message.utf8)
        let bytes_len = UInt16(bytes.count)
        let bytes_len_array = Array([bytes_len])
        
        output.write(bytes_len_array, maxLength: 2)
        output.write(bytes, maxLength: bytes.count)

        var result = Data()
        var buf = [UInt8](repeating: 0, count: 4096)
        while true {
            let n = input.read(&buf, maxLength: buf.count)
            if n <= 0 { break }
            result.append(contentsOf: buf.prefix(n))
        }
        return result.isEmpty ? nil : result
    }
}
