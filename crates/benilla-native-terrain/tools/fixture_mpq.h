// SPDX-License-Identifier: GPL-3.0-or-later
// Authored local-file transport leaf for generated test inputs only. This is not
// an MPQ implementation and must never be used as a production ADT parser.
#pragma once
#include <cstdio>
#include <cstring>
#include <filesystem>
#include <stdexcept>
#include <string>
inline std::string fixture_root;
class MPQFile {
    FILE* file = nullptr;
    size_t length = 0;
public:
    explicit MPQFile(const char* name) {
        constexpr const char* prefix = "DBFilesClient\\";
        std::string path = name;
        if (path.rfind(prefix, 0) == 0)
            path = (std::filesystem::path(fixture_root) / path.substr(std::strlen(prefix))).string();
        file = std::fopen(path.c_str(), "rb");
        if (!file) return;
        std::fseek(file, 0, SEEK_END);
        const long end = std::ftell(file);
        if (end < 0 || end > 2 * 1024 * 1024) throw std::runtime_error("fixture size");
        length = static_cast<size_t>(end);
        std::rewind(file);
    }
    ~MPQFile() { close(); }
    bool isEof() const { return !file || length == 0; }
    size_t getSize() const { return length; }
    size_t read(void* out, size_t count) { return file ? std::fread(out, 1, count, file) : 0; }
    void close() { if (file) { std::fclose(file); file = nullptr; } }
};
