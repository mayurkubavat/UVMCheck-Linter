#include "uvmcheck/uvm/ApiPack.h"

#include "uvmcheck/core/Json.h"

#include <algorithm>
#include <cstdlib>
#include <fstream>
#include <iostream>
#include <mutex>
#include <sstream>

#include "slang/ast/Compilation.h"
#include "slang/ast/symbols/CompilationUnitSymbols.h"
#include "slang/ast/symbols/ParameterSymbols.h"

namespace uvmcheck::uvm {

// Generated at configure time from data/uvm-api/*.json (cmake/EmbedUvmApiPacks.cmake).
struct EmbeddedPack {
    const char* version;
    const char* json;
};
extern const EmbeddedPack kEmbeddedUvmApiPacks[];
extern const size_t kEmbeddedUvmApiPackCount;

namespace {

std::string entryString(const core::json::Value& obj, std::string_view key) {
    const auto* v = obj.find(key);
    return v && v->isString() ? v->asString() : std::string();
}

ApiEntry toEntry(const core::json::Value& obj) {
    return ApiEntry{entryString(obj, "tag"), entryString(obj, "standard"), entryString(obj, "clause")};
}

/// Release line of a version: "2020", "2017" or "1".
std::string lineOf(std::string_view v) {
    if (v.starts_with("2020")) return "2020";
    if (v.starts_with("2017")) return "2017";
    if (v.starts_with("1.")) return "1";
    return std::string(v.substr(0, v.find_first_of(".-")));
}

/// Orders versions so that the newest compares greatest ("2020-2.0" < "2020.3.0").
std::vector<int> versionKey(std::string_view v) {
    std::vector<int> key;
    int cur = -1;
    for (char c : v) {
        if (c >= '0' && c <= '9') cur = (cur < 0 ? 0 : cur * 10) + (c - '0');
        else if (cur >= 0) {
            key.push_back(cur);
            cur = -1;
        }
    }
    if (cur >= 0) key.push_back(cur);
    return key;
}

struct Packs {
    std::vector<std::unique_ptr<ApiPack>> all; // oldest first
};

const Packs& packs() {
    static Packs p = [] {
        Packs out;
        std::string error;
        if (const char* path = std::getenv("UVMCHECK_UVM_API_PACK"); path && *path) {
            std::ifstream in(path, std::ios::binary);
            std::stringstream buf;
            buf << in.rdbuf();
            if (auto pack = ApiPack::parse(buf.str(), error)) out.all.push_back(std::move(pack));
            else std::cerr << "[uvmcheck] warning: UVMCHECK_UVM_API_PACK '" << path << "': " << error << "\n";
            return out;
        }
        for (size_t i = 0; i < kEmbeddedUvmApiPackCount; i++) {
            if (auto pack = ApiPack::parse(kEmbeddedUvmApiPacks[i].json, error)) out.all.push_back(std::move(pack));
        }
        std::sort(out.all.begin(), out.all.end(), [](const auto& a, const auto& b) {
            return versionKey(a->version()) < versionKey(b->version());
        });
        return out;
    }();
    return p;
}

} // namespace

std::string ApiPack::normalizeVersion(std::string_view s) {
    // "Accellera:1800.2:UVM:2020.3.1", "Accellera:1800.2-2020:UVM:2.0", "UVM-1.2"
    std::vector<std::string_view> parts;
    size_t start = 0;
    for (size_t i = 0; i <= s.size(); i++) {
        if (i == s.size() || s[i] == ':') {
            parts.push_back(s.substr(start, i - start));
            start = i + 1;
        }
    }
    std::string_view last = parts.back();
    if (last.starts_with("UVM-")) last.remove_prefix(4);
    std::string v(last);
    if (!v.starts_with("20") && !v.starts_with("1.") && parts.size() > 1) {
        auto dash = parts[1].rfind('-');
        if (dash != std::string_view::npos) v = std::string(parts[1].substr(dash + 1)) + "-" + v;
    }
    return v;
}

std::string ApiPack::detectVersion(const slang::ast::Compilation& compilation) {
    const auto* pkg = compilation.getPackage("uvm_pkg");
    if (!pkg) return {};
    for (std::string_view name : {"UVM_VERSION_STRING", "uvm_revision"}) {
        const auto* sym = pkg->find(name);
        if (!sym || sym->kind != slang::ast::SymbolKind::Parameter) continue;
        const auto& value = sym->as<slang::ast::ParameterSymbol>().getValue();
        if (value.isString()) return normalizeVersion(value.str());
    }
    return {};
}

std::vector<std::string> ApiPack::builtinVersions() {
    std::vector<std::string> out;
    for (const auto& p : packs().all) out.push_back(p->version());
    return out;
}

const ApiPack* ApiPack::forCompilation(const slang::ast::Compilation& compilation) {
    const auto& all = packs().all;
    if (all.empty()) return nullptr;
    const std::string version = detectVersion(compilation);
    if (version.empty()) return all.back().get();
    for (const auto& p : all) {
        if (p->version() == version) return p.get();
    }
    const ApiPack* sameLine = nullptr;
    for (const auto& p : all) {
        if (lineOf(p->version()) == lineOf(version)) sameLine = p.get();
    }
    return sameLine ? sameLine : all.back().get();
}

const ApiEntry* ApiPack::findClass(std::string_view cls) const {
    auto it = classes.find(cls);
    return it == classes.end() || it->second.entry.tag.empty() ? nullptr : &it->second.entry;
}

const ApiEntry* ApiPack::findMember(std::string_view cls, std::string_view member) const {
    auto it = classes.find(cls);
    if (it == classes.end()) return nullptr;
    auto m = it->second.members.find(member);
    return m == it->second.members.end() ? nullptr : &m->second;
}

bool ApiPack::isDeprecatedMacro(std::string_view name) const {
    auto it = macros.find(name);
    return it != macros.end() && it->second;
}

bool ApiPack::hasMacro(std::string_view name) const {
    return macros.find(name) != macros.end();
}

std::unique_ptr<ApiPack> ApiPack::parse(std::string_view json, std::string& error) {
    core::json::ParseError perr;
    auto root = core::json::parse(json, perr);
    if (!root || !root->isObject()) {
        error = "not a JSON object";
        return nullptr;
    }
    auto pack = std::make_unique<ApiPack>();
    pack->versionName = entryString(*root, "version");
    if (pack->versionName.empty()) {
        error = "no \"version\"";
        return nullptr;
    }
    if (const auto* cls = root->find("classes"); cls && cls->isObject()) {
        for (const auto& [name, info] : cls->asObject()) {
            if (!info.isObject()) continue;
            auto& ci = pack->classes[name];
            ci.entry = toEntry(info);
            if (const auto* members = info.find("members"); members && members->isObject()) {
                for (const auto& [mname, m] : members->asObject()) {
                    if (m.isObject()) ci.members.emplace(mname, toEntry(m));
                }
            }
        }
    }
    if (const auto* mac = root->find("macros"); mac && mac->isObject()) {
        for (const auto& [name, info] : mac->asObject()) {
            const auto* dep = info.isObject() ? info.find("deprecated") : nullptr;
            pack->macros.emplace(name, dep && dep->isBool() && dep->asBool());
        }
    }
    return pack;
}

} // namespace uvmcheck::uvm
