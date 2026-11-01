#include "uvmcheck/rules/CompilerDiagnostics.h"
#include "uvmcheck/lsp/LspServer.h"
#include "uvmcheck/core/ConfigManager.h"
#include "uvmcheck/core/Json.h"
#include "uvmcheck/core/ProjectSources.h"
#include "uvmcheck/engine/RuleEngine.h"
#include "uvmcheck/rules/BuiltinRules.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "uvmcheck/uvm/UvmModel.h"
#include "uvmcheck/waivers/WaiverEngine.h"

#include "slang/ast/Compilation.h"
#include "slang/driver/Driver.h"
#include "slang/syntax/SyntaxTree.h"
#include "slang/text/SourceManager.h"

#include <algorithm>
#include <cctype>
#include <cstring>
#include <tuple>
#include <filesystem>
#include <set>
#include <sstream>

namespace uvmcheck::lsp {

namespace fs = std::filesystem;
namespace json = core::json;

namespace {

std::string quote(std::string_view s) {
    std::string out = "\"";
    for (char c : s) {
        switch (c) {
            case '"': out += "\\\""; break;
            case '\\': out += "\\\\"; break;
            case '\n': out += "\\n"; break;
            case '\r': out += "\\r"; break;
            case '\t': out += "\\t"; break;
            default:
                if (static_cast<unsigned char>(c) < 0x20) {
                    char buf[8];
                    std::snprintf(buf, sizeof(buf), "\\u%04x", c);
                    out += buf;
                } else {
                    out += c;
                }
        }
    }
    return out + "\"";
}

/// The request id re-serialized as written (number or string); "null" if absent.
std::string idText(const json::Value& msg) {
    const auto* id = msg.find("id");
    if (!id) return {};
    if (id->isString()) return quote(id->asString());
    if (id->isNumber()) {
        std::ostringstream ss;
        ss << static_cast<long long>(id->asNumber());
        return ss.str();
    }
    return "null";
}

const json::Value* path(const json::Value& v, std::initializer_list<std::string_view> keys) {
    const json::Value* cur = &v;
    for (auto k : keys) {
        cur = cur->find(k);
        if (!cur) return nullptr;
    }
    return cur;
}

std::string str(const json::Value& v, std::initializer_list<std::string_view> keys) {
    const auto* p = path(v, keys);
    return p && p->isString() ? p->asString() : std::string();
}

int num(const json::Value& v, std::initializer_list<std::string_view> keys, int def) {
    const auto* p = path(v, keys);
    return p && p->isNumber() ? static_cast<int>(p->asNumber()) : def;
}

std::string canonical(const std::string& p) {
    std::error_code ec;
    fs::path abs = fs::absolute(p, ec);
    fs::path c = fs::weakly_canonical(abs, ec);
    return (ec ? abs.lexically_normal() : c).string();
}

std::string response(const std::string& id, const std::string& result) {
    return "{\"jsonrpc\":\"2.0\",\"id\":" + id + ",\"result\":" + result + "}";
}

std::string rangeJson(int sl, int sc, int el, int ec) {
    std::ostringstream ss;
    ss << "{\"start\":{\"line\":" << sl << ",\"character\":" << sc << "},\"end\":{\"line\":" << el
       << ",\"character\":" << ec << "}}";
    return ss.str();
}

/// Line `line` (0-based) of `text`.
std::string_view lineOf(std::string_view text, int line) {
    size_t pos = 0;
    for (int i = 0; i < line; ++i) {
        pos = text.find('\n', pos);
        if (pos == std::string_view::npos) return {};
        ++pos;
    }
    size_t end = text.find('\n', pos);
    return text.substr(pos, end == std::string_view::npos ? std::string_view::npos : end - pos);
}

} // namespace

LspServer::LspServer(std::istream& in, std::ostream& out) : inStream(in), outStream(out) {
    rules::registerBuiltinRules();
}

std::string LspServer::uriToPath(const std::string& uri) {
    std::string path = uri;
    if (path.rfind("file://", 0) == 0) path = path.substr(7);
    std::string decoded;
    for (size_t i = 0; i < path.size(); ++i) {
        if (path[i] == '%' && i + 2 < path.size() && std::isxdigit(static_cast<unsigned char>(path[i + 1])) &&
            std::isxdigit(static_cast<unsigned char>(path[i + 2]))) {
            decoded += static_cast<char>(std::stoi(path.substr(i + 1, 2), nullptr, 16));
            i += 2;
            continue;
        }
        decoded += path[i];
    }
    // file:///C:/x -> C:/x on Windows-style URIs
    if (decoded.size() > 2 && decoded[0] == '/' && decoded[2] == ':') decoded.erase(0, 1);
    return decoded;
}

std::string LspServer::pathToUri(const std::string& p) {
    std::string out = "file://";
    for (unsigned char c : fs::path(p).generic_string()) {
        if (std::isalnum(c) || std::strchr("/-_.~:", c)) {
            out += static_cast<char>(c);
        } else {
            char buf[4];
            std::snprintf(buf, sizeof(buf), "%%%02X", c);
            out += buf;
        }
    }
    return out;
}

void LspServer::sendMessage(const std::string& payload) {
    outStream << "Content-Length: " << payload.size() << "\r\n\r\n" << payload;
    outStream.flush();
}

int LspServer::run() {
    std::string headerLine;
    while (!isShutdown && std::getline(inStream, headerLine)) {
        if (headerLine.rfind("Content-Length:", 0) != 0) continue;
        size_t len = std::stoul(headerLine.substr(15));
        std::string line;
        while (std::getline(inStream, line) && line != "\r" && !line.empty()) {
        }
        std::string message(len, '\0');
        inStream.read(message.data(), static_cast<std::streamsize>(len));
        std::string reply = processMessage(message);
        if (!reply.empty()) sendMessage(reply);
    }
    return 0;
}

void LspServer::loadProjectConfig() {
    auto& cfg = core::ConfigManager::instance();
    if (cfg.hasLoadedConfig() || rootPath.empty()) return;
    if (cfg.autoDiscover(rootPath)) {
        std::cerr << "[uvmcheck-lsp] loaded configuration from " << cfg.getLoadedConfigPath() << "\n";
    } else if (!cfg.getLastError().empty()) {
        std::cerr << "[uvmcheck-lsp] config error: " << cfg.getLastError() << "\n";
    }
}

std::string LspServer::processMessage(const std::string& rawJson) {
    json::ParseError perr;
    auto parsed = json::parse(rawJson, perr);
    if (!parsed || !parsed->isObject()) {
        return "{\"jsonrpc\":\"2.0\",\"id\":null,\"error\":{\"code\":-32700,\"message\":\"Parse error\"}}";
    }
    const json::Value& msg = *parsed;
    const std::string method = str(msg, {"method"});
    const std::string id = idText(msg);

    if (method == "initialize") {
        std::string root = str(msg, {"params", "rootUri"});
        if (root.empty()) root = str(msg, {"params", "rootPath"});
        if (!root.empty()) rootPath = uriToPath(root);
        loadProjectConfig();
        return response(id, "{\"capabilities\":{"
                            "\"textDocumentSync\":{\"openClose\":true,\"change\":1,\"save\":true},"
                            "\"codeActionProvider\":{\"codeActionKinds\":[\"quickfix\"]}},"
                            "\"serverInfo\":{\"name\":\"uvmcheck-lsp\",\"version\":\"0.2.0\"}}");
    }

    if (method == "textDocument/didOpen") {
        std::string uri = str(msg, {"params", "textDocument", "uri"});
        if (uri.empty()) return "";
        openDocuments[uri] = str(msg, {"params", "textDocument", "text"});
        lintAndPublish();
        return "";
    }

    if (method == "textDocument/didChange") {
        std::string uri = str(msg, {"params", "textDocument", "uri"});
        const auto* changes = path(msg, {"params", "contentChanges"});
        if (uri.empty() || !changes || !changes->isArray()) return "";
        // Full sync: the last change carries the whole document.
        for (const auto& c : changes->asArray()) {
            if (const auto* t = c.find("text"); t && t->isString()) openDocuments[uri] = t->asString();
        }
        lintAndPublish();
        return "";
    }

    if (method == "textDocument/didSave") {
        std::string uri = str(msg, {"params", "textDocument", "uri"});
        if (const auto* t = path(msg, {"params", "text"}); t && t->isString()) openDocuments[uri] = t->asString();
        lintAndPublish();
        return "";
    }

    if (method == "textDocument/didClose") {
        std::string uri = str(msg, {"params", "textDocument", "uri"});
        openDocuments.erase(uri);
        lastDiagnostics.erase(uri);
        publishDiagnostics(uri, {});
        return "";
    }

    if (method == "textDocument/codeAction") {
        std::string uri = str(msg, {"params", "textDocument", "uri"});
        int first = num(msg, {"params", "range", "start", "line"}, 0);
        int last = num(msg, {"params", "range", "end", "line"}, first);
        return response(id, codeActions(uri, first, last));
    }

    if (method == "shutdown") {
        // Keep reading until "exit", as the protocol requires.
        return response(id, "null");
    }

    if (method == "exit") {
        isShutdown = true;
        return "";
    }

    if (!id.empty()) return response(id, "null"); // unknown request
    return "";                                     // unknown notification
}

void LspServer::lintAndPublish() {
    auto results = lintOpenDocuments();
    for (const auto& [uri, _] : openDocuments) {
        lastDiagnostics[uri] = results[uri];
        publishDiagnostics(uri, lastDiagnostics[uri]);
    }
}

std::map<std::string, std::vector<LspDiagnostic>> LspServer::lintOpenDocuments() {
    std::map<std::string, std::vector<LspDiagnostic>> out;
    if (openDocuments.empty()) return out;

    auto& cfgMgr = core::ConfigManager::instance();
    const auto& projectConfig = cfgMgr.getProjectConfig();
    const auto& build = projectConfig.build();

    slang::driver::Driver driver;
    driver.addStandardArgs();
    (void)core::applyBuildSettings(driver, build, build.hasSources());
    auto& sm = driver.sourceManager;

    // The editor's text stands in for each open file: assigning it first makes
    // the loader and `include lookups find it in the source manager's cache.
    std::map<std::string, std::string> uriByPath;            // canonical path -> uri
    std::map<std::string, slang::SourceBuffer> overlayByPath; // canonical path -> buffer
    for (const auto& [uri, text] : openDocuments) {
        std::string p = canonical(uriToPath(uri));
        if (uriByPath.count(p)) continue;
        uriByPath[p] = uri;
        try {
            overlayByPath[p] = sm.assignText(p, text);
        } catch (const std::exception& e) {
            std::cerr << "[uvmcheck-lsp] " << p << ": " << e.what() << "\n";
        }
    }

    if (!driver.processOptions(/* checkFiles */ false)) {
        std::cerr << "[uvmcheck-lsp] invalid build settings in project config\n";
    }
    (void)driver.parseAllSources();

    // Open files that the project does not compile (or every open file when
    // the config names no sources) are parsed as files of their own.
    std::set<std::string> loaded;
    for (const auto& tree : driver.syntaxTrees) {
        for (auto bufId : tree->getSourceBufferIds()) loaded.insert(canonical(std::string(sm.getFullPath(bufId).string())));
    }
    auto parseOptions = driver.createParseOptionBag();
    for (const auto& [p, buffer] : overlayByPath) {
        if (loaded.count(p)) continue;
        driver.syntaxTrees.push_back(slang::syntax::SyntaxTree::fromBuffer(buffer, sm, parseOptions));
    }

    auto compilation = driver.createCompilation();
    rules::CompilerDiagnosticEngineScope compilerEngineScope(driver.diagEngine);
    (void)compilation->getAllDiagnostics();
    uvm::UvmModel uvmModel(*compilation);

    core::DiagnosticEngine diags(sm);
    diags.setProjectConfig(&projectConfig);

    // Same default as the CLI: opt-in Advisory rules stay off in the editor.
    const auto advisory = static_cast<uint32_t>(rules::RuleProfile::Advisory);
    std::vector<rules::Rule*> rules;
    for (const auto& rule : rules::RuleRegistry::instance().getRules()) {
        if ((rule->getProfiles() & ~advisory) == 0) continue;
        if (rule->getMaturity() == rules::RuleMaturity::Experimental) continue;
        if (!projectConfig.isRuleEnabled(rule->getId(), rule->getName())) continue;
        rules.push_back(rule.get());
    }
    engine::RuleEngine ruleEngine(*compilation, uvmModel, sm, engine::EngineOptions{});
    ruleEngine.run(rules, diags);

    // Waivers and baseline, as on the command line.
    fs::path projectRoot = rootPath.empty() ? fs::current_path() : fs::path(rootPath);
    if (cfgMgr.hasLoadedConfig()) {
        fs::path dir = fs::absolute(cfgMgr.getLoadedConfigPath()).parent_path();
        projectRoot = dir.filename() == ".uvmcheck" ? dir.parent_path() : dir;
    }
    waivers::WaiverEngine waiverEngine(sm);
    waiverEngine.setProjectRoot(projectRoot);
    for (const auto& cw : cfgMgr.getWaivers()) {
        waivers::ExternalWaiver w;
        w.ruleId = cw.ruleId;
        w.line = cw.line;
        w.reason = cw.reason;
        w.glob = cw.path;
        waiverEngine.addWaiver(std::move(w));
    }
    std::error_code ec;
    if (fs::exists(projectRoot / ".uvmcheck/waivers.json", ec)) {
        (void)waiverEngine.loadWaivers((projectRoot / ".uvmcheck/waivers.json").string());
    }
    if (fs::exists(projectRoot / ".uvmcheck/baseline.json", ec)) {
        (void)waiverEngine.loadBaseline((projectRoot / ".uvmcheck/baseline.json").string());
    }

    // Map each finding to an open document; inline waivers on the line above
    // must read the editor's text, not the file on disk.
    std::map<std::string, std::string> pathOfName; // violation file name -> canonical path
    for (const auto& v : diags.getViolations()) {
        if (v.filePath.empty() || pathOfName.count(v.filePath)) continue;
        std::string p = canonical(v.filePath);
        pathOfName[v.filePath] = p;
        if (auto it = uriByPath.find(p); it != uriByPath.end()) {
            waiverEngine.setFileText(v.filePath, openDocuments[it->second]);
        }
    }

    for (const auto& v : waiverEngine.filter(diags.getViolations())) {
        if (v.filePath.empty()) continue;
        auto it = uriByPath.find(pathOfName[v.filePath]);
        if (it == uriByPath.end()) continue;

        LspDiagnostic d;
        d.line = v.line > 0 ? static_cast<int>(v.line) - 1 : 0;
        d.startChar = v.column > 0 ? static_cast<int>(v.column) - 1 : 0;
        d.endChar = v.endColumn > v.column ? static_cast<int>(v.endColumn) - 1 : d.startChar + 1;
        d.severity = v.severity == core::Severity::Error ? 1 : v.severity == core::Severity::Info ? 3 : 2;
        d.code = v.ruleId;
        d.ruleName = v.ruleName;
        d.message = v.message;

        for (const auto& r : v.related) {
            if (r.filePath.empty() || r.line == 0) continue;
            d.related.push_back({pathToUri(canonical(r.filePath)), static_cast<int>(r.line) - 1,
                                 r.column > 0 ? static_cast<int>(r.column) - 1 : 0, r.message});
        }
        for (const auto& fix : v.fixIts) {
            auto s = fix.range.start(), e = fix.range.end();
            if (!s.valid() || !e.valid() || sm.isMacroLoc(s) || sm.isMacroLoc(e)) continue;
            if (canonical(std::string(sm.getFullPath(s.buffer()).string())) != it->first) continue;
            LspFix f;
            f.title = fix.description.empty() ? "Fix " + v.ruleId : fix.description;
            f.edit.startLine = static_cast<int>(sm.getLineNumber(s)) - 1;
            f.edit.startChar = static_cast<int>(sm.getColumnNumber(s)) - 1;
            f.edit.endLine = static_cast<int>(sm.getLineNumber(e)) - 1;
            f.edit.endChar = static_cast<int>(sm.getColumnNumber(e)) - 1;
            f.edit.newText = fix.replacementText;
            d.fixes.push_back(std::move(f));
        }
        out[it->second].push_back(std::move(d));
    }
    for (auto& [_, list] : out) {
        std::stable_sort(list.begin(), list.end(), [](const LspDiagnostic& a, const LspDiagnostic& b) {
            return std::tie(a.line, a.startChar) < std::tie(b.line, b.startChar);
        });
    }
    return out;
}

void LspServer::publishDiagnostics(const std::string& uri, const std::vector<LspDiagnostic>& diags) {
    std::ostringstream ss;
    ss << "{\"jsonrpc\":\"2.0\",\"method\":\"textDocument/publishDiagnostics\",\"params\":{\"uri\":"
       << quote(uri) << ",\"diagnostics\":[";
    for (size_t i = 0; i < diags.size(); ++i) {
        const auto& d = diags[i];
        ss << (i ? "," : "") << "{\"range\":" << rangeJson(d.line, d.startChar, d.line, d.endChar)
           << ",\"severity\":" << d.severity << ",\"code\":" << quote(d.code)
           << ",\"source\":\"uvmcheck\",\"message\":" << quote(d.message);
        if (!d.related.empty()) {
            ss << ",\"relatedInformation\":[";
            for (size_t r = 0; r < d.related.size(); ++r) {
                const auto& rl = d.related[r];
                ss << (r ? "," : "") << "{\"location\":{\"uri\":" << quote(rl.uri) << ",\"range\":"
                   << rangeJson(rl.line, rl.character, rl.line, rl.character) << "},\"message\":"
                   << quote(rl.message) << "}";
            }
            ss << "]";
        }
        ss << "}";
    }
    ss << "]}}";
    sendMessage(ss.str());
}

std::string LspServer::codeActions(const std::string& uri, int firstLine, int lastLine) const {
    auto it = lastDiagnostics.find(uri);
    if (it == lastDiagnostics.end()) return "[]";
    std::string_view text;
    if (auto doc = openDocuments.find(uri); doc != openDocuments.end()) text = doc->second;

    auto editJson = [&](const LspEdit& e) {
        return "{\"changes\":{" + quote(uri) + ":[{\"range\":" +
               rangeJson(e.startLine, e.startChar, e.endLine, e.endChar) +
               ",\"newText\":" + quote(e.newText) + "}]}}";
    };
    auto diagJson = [&](const LspDiagnostic& d) {
        return "{\"range\":" + rangeJson(d.line, d.startChar, d.line, d.endChar) +
               ",\"code\":" + quote(d.code) + ",\"source\":\"uvmcheck\",\"message\":" + quote(d.message) + "}";
    };

    std::vector<std::string> actions;
    std::set<std::pair<int, std::string>> waived;
    for (const auto& d : it->second) {
        if (d.line < firstLine || d.line > lastLine) continue;
        for (const auto& f : d.fixes) {
            actions.push_back("{\"title\":" + quote(f.title) + ",\"kind\":\"quickfix\",\"isPreferred\":true,"
                              "\"diagnostics\":[" + diagJson(d) + "],\"edit\":" + editJson(f.edit) + "}");
        }
        if (!waived.insert({d.line, d.code}).second) continue;
        std::string_view line = lineOf(text, d.line);
        std::string indent(line.substr(0, line.find_first_not_of(" \t") == std::string_view::npos
                                              ? 0 : line.find_first_not_of(" \t")));
        LspEdit e;
        e.startLine = e.endLine = d.line;
        e.newText = indent + "// uvmcheck:waive-next-line " + d.code + " -- reason: \n";
        actions.push_back("{\"title\":" + quote("Waive " + d.code + " on this line") +
                          ",\"kind\":\"quickfix\",\"diagnostics\":[" + diagJson(d) + "],\"edit\":" +
                          editJson(e) + "}");
    }
    std::string out = "[";
    for (size_t i = 0; i < actions.size(); ++i) out += (i ? "," : "") + actions[i];
    return out + "]";
}

} // namespace uvmcheck::lsp
