#include "SocketClient.h"
#include <winsock2.h>
#include <ws2tcpip.h>
#include <string>
#include <vector>
#include "json.hpp"
#include "Logger.h"

using json = nlohmann::json;

#pragma comment(lib, "ws2_32.lib")

int SocketClient::last_status = 0;

std::string SocketClient::wideToUtf8(const std::wstring& w) {
    if (w.empty()) return {};
    int n = WideCharToMultiByte(CP_UTF8, 0, w.c_str(), -1, nullptr, 0, nullptr, nullptr);
    std::string result(n - 1, '\0');
    WideCharToMultiByte(CP_UTF8, 0, w.c_str(), -1, result.data(), n, nullptr, nullptr);
    return result;
}

std::wstring SocketClient::utf8ToWide(const std::string& s) {
    if (s.empty()) return {};
    int n = MultiByteToWideChar(CP_UTF8, 0, s.c_str(), -1, nullptr, 0);
    std::wstring result(n - 1, L'\0');
    MultiByteToWideChar(CP_UTF8, 0, s.c_str(), -1, result.data(), n);
    return result;
}

std::string SocketClient::sendReceive(const std::string& message) {
    WSADATA wsaData;

    if (WSAStartup(MAKEWORD(2, 2), &wsaData) != 0) return {};

    SOCKET sock = socket(AF_INET, SOCK_STREAM, IPPROTO_TCP);
    if (sock == INVALID_SOCKET) { WSACleanup(); return {}; }

    // SO_RCVTIMEO/SO_SNDTIMEO don't affect connect() on Windows,
    // so use non-blocking + select() for a real connect timeout.
    u_long nonBlocking = 1;
    ioctlsocket(sock, FIONBIO, &nonBlocking);

    sockaddr_in addr{};
    addr.sin_family = AF_INET;
    addr.sin_port = htons(PORT);
    inet_pton(AF_INET, "127.0.0.1", &addr.sin_addr);

    connect(sock, (sockaddr*)&addr, sizeof(addr)); // returns WSAEWOULDBLOCK immediately

    fd_set wset;
    FD_ZERO(&wset);
    FD_SET(sock, &wset);
    timeval tv{ 0, 50'000 }; // 50ms
    if (select(0, nullptr, &wset, nullptr, &tv) != 1) {
        Log(__FUNCTION__, L"not connecting");
        closesocket(sock); WSACleanup(); last_status = -1; return {};
    }

    // Back to blocking for send/recv
    u_long blocking = 0;
    ioctlsocket(sock, FIONBIO, &blocking);

    DWORD timeout = 200;
    setsockopt(sock, SOL_SOCKET, SO_RCVTIMEO, (char*)&timeout, sizeof(timeout));
    setsockopt(sock, SOL_SOCKET, SO_SNDTIMEO, (char*)&timeout, sizeof(timeout));

    uint16_t len = static_cast<uint16_t>(message.size());
    send(sock, reinterpret_cast<char*>(&len), 2, 0);
    send(sock, message.c_str(), static_cast<int>(message.size()), 0);

    std::string result;
    char buf[4096];
    int n;
    while ((n = recv(sock, buf, sizeof(buf), 0)) > 0)
        result.append(buf, n);

    closesocket(sock);
    WSACleanup();

    if (result == "loading") {
        last_status = 1;
        return {};
    }else if ( result == "error" ) {
        last_status = -1;
        return {};
    }

    last_status = 0;
    return result;
}

std::vector<FileRevision> SocketClient::parseRevisions(const std::string& jsonStr) {
    std::vector<FileRevision> revisions;
    try {
        for (const auto& obj : json::parse(jsonStr)) {
            FileRevision rev;
            rev.id               = utf8ToWide(obj.value("id", ""));
            rev.fileId           = utf8ToWide(obj.value("fileId", ""));
            rev.modifiedTime     = utf8ToWide(obj.value("modifiedTime", ""));
            rev.size             = utf8ToWide(obj.value("size", ""));
            rev.originalFilename = utf8ToWide(obj.value("originalFilename", ""));
            revisions.push_back(rev);
        }
    } catch (...) {}
    return revisions;
}

std::string SocketClient::serializeRevisions(const std::vector<FileRevision>& revisions) {
    json arr = json::array();
    for (const auto& rev : revisions) {
        arr.push_back({
            {"id",               wideToUtf8(rev.id)},
            {"fileId",           wideToUtf8(rev.fileId)},
            {"modifiedTime",     wideToUtf8(rev.modifiedTime)},
            {"size",             wideToUtf8(rev.size)},
            {"originalFilename", wideToUtf8(rev.originalFilename)},
        });
    }
    return arr.dump();
}

std::vector<FileRevision> SocketClient::getRevisions(const std::wstring& path, bool refresh) {
    
    std::string json_string = sendReceive((refresh ? "refresh@@" :"revisions@@") + wideToUtf8(path));
    if (last_status != 0) {
        return {};
    }
    return parseRevisions(json_string);
}

bool SocketClient::downloadFile(const std::wstring& fileId,
                                const std::wstring& revisionId,
                                const std::wstring& modifiedTime) {
    std::string msg = "download@@"
        + wideToUtf8(fileId) + "@@"
        + wideToUtf8(revisionId) + "@@"
        + wideToUtf8(modifiedTime);

    sendReceive(msg);
    return last_status == 0;
}

void SocketClient::showAllRevisions(const std::wstring& path) {
    std::string msg = "all@@"
        + wideToUtf8(path);

    sendReceive(msg);
}
