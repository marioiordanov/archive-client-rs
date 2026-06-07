#pragma once
#include <windows.h>
#include <string>

inline void Log(const char* func, const std::wstring& msg) {
    wchar_t tempPath[MAX_PATH];
    GetTempPathW(MAX_PATH, tempPath);
    std::wstring logPath = std::wstring(tempPath) + L"ArchiveContextMenu.log";

    SYSTEMTIME st;
    GetLocalTime(&st);

    int msgLen = WideCharToMultiByte(CP_UTF8, 0, msg.c_str(), -1, nullptr, 0, nullptr, nullptr);
    std::string msgUtf8(msgLen > 0 ? msgLen - 1 : 0, '\0');
    if (msgLen > 0)
        WideCharToMultiByte(CP_UTF8, 0, msg.c_str(), -1, msgUtf8.data(), msgLen, nullptr, nullptr);

    char line[4096];
    int lineLen = _snprintf_s(line, sizeof(line), _TRUNCATE,
        "[%02d:%02d:%02d.%03d] %s: %s\r\n",
        st.wHour, st.wMinute, st.wSecond, st.wMilliseconds,
        func, msgUtf8.c_str());
    if (lineLen <= 0) return;

    HANDLE hFile = CreateFileW(logPath.c_str(), FILE_APPEND_DATA,
        FILE_SHARE_READ | FILE_SHARE_WRITE, nullptr,
        OPEN_ALWAYS, FILE_ATTRIBUTE_NORMAL, nullptr);
    if (hFile == INVALID_HANDLE_VALUE) return;

    DWORD written;
    WriteFile(hFile, line, static_cast<DWORD>(lineLen), &written, nullptr);
    CloseHandle(hFile);
}
