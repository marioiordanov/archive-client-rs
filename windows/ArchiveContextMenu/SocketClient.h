#pragma once
#include <windows.h>
#include <string>
#include <vector>

struct FileRevision {
    std::wstring id;
    std::wstring fileId;
    std::wstring modifiedTime;
    std::wstring size;
    std::wstring originalFilename;

    std::wstring displayTitle() const {
        return originalFilename + L" (" + modifiedTime + L")";
    }
};

class SocketClient {
public:
    static const int PORT = 38787;
    static int last_status;

    static std::vector<FileRevision> getRevisions(const std::wstring& path, bool refresh=false);
    static bool downloadFile(const std::wstring& fileId,
                             const std::wstring& revisionId,
                             const std::wstring& modifiedTime);

    static void showAllRevisions(const std::wstring& path);

    static std::vector<FileRevision> parseRevisions(const std::string& json);
    static std::string serializeRevisions(const std::vector<FileRevision>& revisions);

    static std::wstring utf8ToWide(const std::string& s);
    static std::string  wideToUtf8(const std::wstring& w);

private:
    static std::string sendReceive(const std::string& message);
};
