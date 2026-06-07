#include "ContextMenuHandler.h"
#include <shellapi.h>
#include <fstream>
#include <filesystem>
#include <fileapi.h>
#include "json.hpp"
#include "Logger.h"
#include <cstdlib>

extern LONG      g_dllRefCount;
extern HINSTANCE g_hInstance;
const UINT MAX_ELEMENTS_SHOWN = 3;

// ---- ContextMenuHandler: IUnknown ------------------------------------------

STDMETHODIMP ContextMenuHandler::QueryInterface(REFIID riid, void** ppv) {
    if (riid == IID_IUnknown || riid == IID_IShellExtInit)
        *ppv = static_cast<IShellExtInit*>(this);
    else if (riid == IID_IContextMenu)
        *ppv = static_cast<IContextMenu*>(this);
    else { *ppv = nullptr; return E_NOINTERFACE; }
    AddRef();
    return S_OK;
}

STDMETHODIMP_(ULONG) ContextMenuHandler::AddRef()  { return InterlockedIncrement(&m_refCount); }
STDMETHODIMP_(ULONG) ContextMenuHandler::Release() {
    LONG r = InterlockedDecrement(&m_refCount);
    if (r == 0) { InterlockedDecrement(&g_dllRefCount); delete this; }
    return r;
}

// ---- ContextMenuHandler: IShellExtInit -------------------------------------

STDMETHODIMP ContextMenuHandler::Initialize(PCIDLIST_ABSOLUTE, IDataObject* pdtobj, HKEY) {
    if (!pdtobj) return E_INVALIDARG;
    FORMATETC fmt = { CF_HDROP, nullptr, DVASPECT_CONTENT, -1, TYMED_HGLOBAL };
    STGMEDIUM stg = {};
    if (FAILED(pdtobj->GetData(&fmt, &stg))) return E_FAIL;
    HDROP hDrop = static_cast<HDROP>(GlobalLock(stg.hGlobal));
    if (hDrop) {
        WCHAR path[MAX_PATH] = {};
        if (DragQueryFileW(hDrop, 0, path, MAX_PATH))
            m_filePath = path;
        GlobalUnlock(stg.hGlobal);
    }
    ReleaseStgMedium(&stg);
    return m_filePath.empty() ? E_FAIL : S_OK;
}

// ---- ContextMenuHandler: IContextMenu --------------------------------------

static std::wstring GetMappedDir() {
    PWSTR appdata = NULL;
    SHGetKnownFolderPath((const KNOWNFOLDERID)FOLDERID_RoamingAppData, KF_FLAG_DEFAULT_PATH, NULL, &appdata);
    Log(__FUNCTION__, appdata);

    namespace fs = std::filesystem;
    // DLL location = dllPath's parent; go 2 more levels up, then look for config
    fs::path configPath = fs::path(appdata) / L"archive-client-rs" / L"org.json";
    Log(__FUNCTION__, configPath.c_str());

    std::ifstream f(configPath, std::ifstream::in);
    if (!f.is_open()) return {};
     Log(__FUNCTION__, L"file in place read");

    try {
        auto config = nlohmann::json::parse(f);
        Log(__FUNCTION__, L"config parsed");
        if (!config.contains("config")) return {};
        std::string utf8 = config["config"]["local_folder_path"].get<std::string>();
        return fs::u8path(utf8).wstring();
    } catch (...) {
        Log(__FUNCTION__, L"exception");
        return {};
    }
}

STDMETHODIMP ContextMenuHandler::QueryContextMenu(HMENU hmenu, UINT indexMenu,
                                                   UINT idCmdFirst, UINT, UINT uFlags) {
    if (uFlags & CMF_DEFAULTONLY) return MAKE_HRESULT(SEVERITY_SUCCESS, 0, 0);

    std::wstring mappedDir = GetMappedDir();
    if (mappedDir.empty()) return MAKE_HRESULT(SEVERITY_SUCCESS, 0, 0);

    DWORD attributes = GetFileAttributes(m_filePath.c_str());
    if (attributes == INVALID_FILE_ATTRIBUTES || (attributes & FILE_ATTRIBUTE_DIRECTORY)) {
        return MAKE_HRESULT(SEVERITY_SUCCESS, 0, 0);
    }

    if (mappedDir.back() != L'\\' && mappedDir.back() != L'/') {
        mappedDir += L'\\';
    }
   
    if (wcsncmp(mappedDir.c_str(), m_filePath.c_str(), mappedDir.length()) != 0) {
        return MAKE_HRESULT(SEVERITY_SUCCESS, 0, 0);
    }

    mappedDir += L".archived\\";

    if (wcsncmp(mappedDir.c_str(), m_filePath.c_str(), mappedDir.length()) == 0) {
        return MAKE_HRESULT(SEVERITY_SUCCESS, 0, 0);
    }
    
    m_revisions = SocketClient::getRevisions(m_filePath);

    if (SocketClient::last_status == -1) {
         return MAKE_HRESULT(SEVERITY_SUCCESS, 0, 0);
    }

    HMENU hSub = CreatePopupMenu();
    if (!hSub) return MAKE_HRESULT(SEVERITY_SUCCESS, 0, 0);

    if (SocketClient::last_status == 1) {
        InsertMenuW(hSub, 0, MF_BYPOSITION | MF_STRING | MF_GRAYED, 0, L"Loading");
    } else {
        if (m_revisions.empty()) {
            InsertMenuW(hSub, 0, MF_BYPOSITION | MF_STRING | MF_GRAYED, 0, L"No items found");
        } else {
            m_shownCount = min(static_cast<UINT>(m_revisions.size()), MAX_ELEMENTS_SHOWN);
            for (UINT i = 0; i < m_shownCount; i++) {
                InsertMenuW(hSub, i, MF_BYPOSITION | MF_STRING,
                            idCmdFirst + i, m_revisions[i].displayTitle().c_str());
            }

            InsertMenuW(hSub, m_shownCount,     MF_BYPOSITION | MF_SEPARATOR, 0, nullptr);
            InsertMenuW(hSub, m_shownCount + 1, MF_BYPOSITION | MF_STRING,
                        idCmdFirst + m_shownCount, L"Refresh");
            
            if (m_revisions.size() > m_shownCount) {
                InsertMenuW(hSub, m_shownCount + 2, MF_BYPOSITION | MF_STRING,
                        idCmdFirst + m_shownCount + 1, L"Show All");
            }
        }
    }

    InsertMenuW(hmenu, indexMenu, MF_BYPOSITION | MF_POPUP | MF_STRING,
                reinterpret_cast<UINT_PTR>(hSub), L"Archived versions");

    return MAKE_HRESULT(SEVERITY_SUCCESS, 0, m_shownCount +2 + 1);
}

STDMETHODIMP ContextMenuHandler::InvokeCommand(CMINVOKECOMMANDINFO* pici) {
    if (HIWORD(pici->lpVerb) != 0) return E_INVALIDARG;

    UINT idx = LOWORD(pici->lpVerb);

    if (idx == m_shownCount) {
        Log(__FUNCTION__, L"Refresh clicked");
        m_revisions = SocketClient::getRevisions(m_filePath, true);
        return S_OK;
    }

    if (idx == m_shownCount + 1) {
        Log(__FUNCTION__, L"Show All clicked");
        SocketClient::showAllRevisions(m_filePath);
        return S_OK;
    }

    if (idx >= m_shownCount) return E_INVALIDARG;

    const FileRevision& rev = m_revisions[idx];
    Log(__FUNCTION__, L"revision clicked: " + rev.displayTitle());
    SocketClient::downloadFile(rev.fileId, rev.id, rev.modifiedTime);
    return S_OK;
}


STDMETHODIMP ContextMenuHandler::GetCommandString(UINT_PTR, UINT, UINT*, CHAR*, UINT) {
    return S_OK;
}

// ---- ClassFactory ----------------------------------------------------------

STDMETHODIMP ClassFactory::QueryInterface(REFIID riid, void** ppv) {
    if (riid == IID_IUnknown || riid == IID_IClassFactory)
        { *ppv = this; AddRef(); return S_OK; }
    *ppv = nullptr; return E_NOINTERFACE;
}

STDMETHODIMP_(ULONG) ClassFactory::AddRef()  { return InterlockedIncrement(&m_refCount); }
STDMETHODIMP_(ULONG) ClassFactory::Release() {
    LONG r = InterlockedDecrement(&m_refCount);
    if (r == 0) delete this;
    return r;
}

STDMETHODIMP ClassFactory::CreateInstance(IUnknown* pUnkOuter, REFIID riid, void** ppv) {
    if (pUnkOuter) return CLASS_E_NOAGGREGATION;
    auto* p = new (std::nothrow) ContextMenuHandler();
    if (!p) return E_OUTOFMEMORY;
    InterlockedIncrement(&g_dllRefCount);
    HRESULT hr = p->QueryInterface(riid, ppv);
    p->Release();
    return hr;
}

STDMETHODIMP ClassFactory::LockServer(BOOL fLock) {
    fLock ? InterlockedIncrement(&g_dllRefCount) : InterlockedDecrement(&g_dllRefCount);
    return S_OK;
}
