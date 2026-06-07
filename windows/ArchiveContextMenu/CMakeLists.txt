cmake_minimum_required(VERSION 3.20)
project(ArchiveContextMenu)

add_library(ArchiveContextMenu SHARED
    SocketClient.cpp
    dllmain.cpp
    ContextMenuHandler.cpp
    ArchiveContextMenu.def
)

target_compile_features(ArchiveContextMenu PRIVATE cxx_std_17)

target_link_libraries(ArchiveContextMenu PRIVATE
    shlwapi
    ws2_32
)

# Required for COM shell extensions
target_compile_definitions(ArchiveContextMenu PRIVATE
    WIN32_LEAN_AND_MEAN
    UNICODE
    _UNICODE
)

# Install target: copies the DLL next to the main .exe
install(TARGETS ArchiveContextMenu RUNTIME DESTINATION .)
