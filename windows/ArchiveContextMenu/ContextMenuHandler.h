#pragma once
#include <windows.h>
#include <shlobj.h>
#include <string>
#include <vector>
#include "SocketClient.h"

// {EBEF468F-C51E-48BB-AFB4-C3F021B1FA3E}
static constexpr GUID CLSID_ArchiveContextMenu =
    { 0xEBEF468F, 0xC51E, 0x48BB, { 0xAF, 0xB4, 0xC3, 0xF0, 0x21, 0xB1, 0xFA, 0x3E } };

class ContextMenuHandler : public IShellExtInit, public IContextMenu {
public:
    STDMETHOD(QueryInterface)(REFIID riid, void** ppv) override;
    STDMETHOD_(ULONG, AddRef)() override;
    STDMETHOD_(ULONG, Release)() override;

    STDMETHOD(Initialize)(PCIDLIST_ABSOLUTE pidlFolder, IDataObject* pdtobj, HKEY hkeyProgID) override;
    STDMETHOD(QueryContextMenu)(HMENU hmenu, UINT indexMenu, UINT idCmdFirst, UINT idCmdLast, UINT uFlags) override;
    STDMETHOD(InvokeCommand)(CMINVOKECOMMANDINFO* pici) override;
    STDMETHOD(GetCommandString)(UINT_PTR idCmd, UINT uType, UINT* pReserved, CHAR* pszName, UINT cchMax) override;

private:
    LONG                      m_refCount = 1;
    std::wstring              m_filePath;
    std::vector<FileRevision> m_revisions;
    UINT                      m_shownCount = 0; // revisions actually inserted (max 3)
};

class ClassFactory : public IClassFactory {
public:
    STDMETHOD(QueryInterface)(REFIID riid, void** ppv) override;
    STDMETHOD_(ULONG, AddRef)() override;
    STDMETHOD_(ULONG, Release)() override;
    STDMETHOD(CreateInstance)(IUnknown* pUnkOuter, REFIID riid, void** ppv) override;
    STDMETHOD(LockServer)(BOOL fLock) override;
private:
    LONG m_refCount = 1;
};
