#include <windows.h>
#include <shlobj.h>
#include <strsafe.h>
#include "ContextMenuHandler.h"

HINSTANCE g_hInstance = nullptr;
LONG g_dllRefCount = 0;

static const wchar_t* CLSID_STR = L"{EBEF468F-C51E-48BB-AFB4-C3F021B1FA3E}";

BOOL APIENTRY DllMain(HMODULE hModule, DWORD reason, LPVOID /*reserved*/) {
    if (reason == DLL_PROCESS_ATTACH) {
        g_hInstance = hModule;
        DisableThreadLibraryCalls(hModule);
    }
    return TRUE;
}

STDAPI DllGetClassObject(REFCLSID rclsid, REFIID riid, void** ppv) {
    if (rclsid != CLSID_ArchiveContextMenu) return CLASS_E_CLASSNOTAVAILABLE;
    auto* cf = new (std::nothrow) ClassFactory();
    if (!cf) return E_OUTOFMEMORY;
    HRESULT hr = cf->QueryInterface(riid, ppv);
    cf->Release();
    return hr;
}

STDAPI DllCanUnloadNow() {
    return (g_dllRefCount == 0) ? S_OK : S_FALSE;
}

static HRESULT setRegValue(HKEY hRoot, const wchar_t* subKey,
                           const wchar_t* name, const wchar_t* value) {
    HKEY hKey;
    LONG r = RegCreateKeyExW(hRoot, subKey, 0, nullptr,
                             REG_OPTION_NON_VOLATILE, KEY_SET_VALUE,
                             nullptr, &hKey, nullptr);
    if (r != ERROR_SUCCESS) return HRESULT_FROM_WIN32(r);
    r = RegSetValueExW(hKey, name, 0, REG_SZ,
                       (const BYTE*)value,
                       static_cast<DWORD>((wcslen(value) + 1) * sizeof(wchar_t)));
    RegCloseKey(hKey);
    return HRESULT_FROM_WIN32(r);
}

STDAPI DllRegisterServer() {
    wchar_t dllPath[MAX_PATH];
    GetModuleFileNameW(g_hInstance, dllPath, MAX_PATH);

    wchar_t key[256];

    StringCchPrintfW(key, 256, L"CLSID\\%s", CLSID_STR);
    setRegValue(HKEY_CLASSES_ROOT, key, nullptr, L"Archive Context Menu");
    setRegValue(HKEY_CLASSES_ROOT, key, L"AppID", CLSID_STR); // run in dllhost.exe surrogate

    StringCchPrintfW(key, 256, L"CLSID\\%s\\InProcServer32", CLSID_STR);
    setRegValue(HKEY_CLASSES_ROOT, key, nullptr, dllPath);
    setRegValue(HKEY_CLASSES_ROOT, key, L"ThreadingModel", L"Apartment");

    // Empty DllSurrogate value = use the default dllhost.exe surrogate process.
    // Explorer calls the extension out-of-process, so it never locks the DLL file.
    StringCchPrintfW(key, 256, L"AppID\\%s", CLSID_STR);
    setRegValue(HKEY_CLASSES_ROOT, key, L"DllSurrogate", L"");

    StringCchPrintfW(key, 256, L"*\\shellex\\ContextMenuHandlers\\ArchiveContextMenu");
    setRegValue(HKEY_CLASSES_ROOT, key, nullptr, CLSID_STR);

    StringCchPrintfW(key, 256, L"Directory\\shellex\\ContextMenuHandlers\\ArchiveContextMenu");
    setRegValue(HKEY_CLASSES_ROOT, key, nullptr, CLSID_STR);

    setRegValue(HKEY_LOCAL_MACHINE,
        L"SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Shell Extensions\\Approved",
        CLSID_STR, L"Archive Context Menu");

    SHChangeNotify(SHCNE_ASSOCCHANGED, SHCNF_IDLIST, nullptr, nullptr);
    return S_OK;
}

STDAPI DllUnregisterServer() {
    wchar_t keyPath[256];

    StringCchPrintfW(keyPath, 256,
        L"*\\shellex\\ContextMenuHandlers\\ArchiveContextMenu");
    RegDeleteKeyW(HKEY_CLASSES_ROOT, keyPath);

    StringCchPrintfW(keyPath, 256,
        L"Directory\\shellex\\ContextMenuHandlers\\ArchiveContextMenu");
    RegDeleteKeyW(HKEY_CLASSES_ROOT, keyPath);

    StringCchPrintfW(keyPath, 256, L"CLSID\\%s\\InProcServer32", CLSID_STR);
    RegDeleteKeyW(HKEY_CLASSES_ROOT, keyPath);
    StringCchPrintfW(keyPath, 256, L"CLSID\\%s", CLSID_STR);
    RegDeleteKeyW(HKEY_CLASSES_ROOT, keyPath);
    StringCchPrintfW(keyPath, 256, L"AppID\\%s", CLSID_STR);
    RegDeleteKeyW(HKEY_CLASSES_ROOT, keyPath);

    HKEY hApproved;
    if (RegOpenKeyExW(HKEY_LOCAL_MACHINE,
            L"SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Shell Extensions\\Approved",
            0, KEY_SET_VALUE, &hApproved) == ERROR_SUCCESS) {
        RegDeleteValueW(hApproved, CLSID_STR);
        RegCloseKey(hApproved);
    }

    SHChangeNotify(SHCNE_ASSOCCHANGED, SHCNF_IDLIST, nullptr, nullptr);
    return S_OK;
}
