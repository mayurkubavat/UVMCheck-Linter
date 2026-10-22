#include "uvmcheck/lsp/LspServer.h"
#include "uvmcheck/core/ConfigManager.h"
#include "uvmcheck/engine/RuleEngine.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "uvmcheck/rules/BuiltinRules.h"
#include "uvmcheck/uvm/UvmModel.h"
#include "slang/syntax/SyntaxTree.h"
#include "slang/ast/Compilation.h"
#include "slang/text/SourceManager.h"
#include <sstream>
#include <fstream>
#include <algorithm>

namespace uvmcheck::lsp {

static std::string escapeJson(std::string_view s) {
    std::string out;
    for (char c : s) {
        if (c == '"') out += "\\\"";
        else if (c == '\\') out += "\\\\";
        else if (c == '\n') out += "\\n";
        else if (c == '\r') out += "\\r";
        else if (c == '\t') out += "\\t";
        else out += c;
    }
    return out;
}

static std::string extractStringField(const std::string& json, const std::string& fieldName) {
    std::string search = "\"" + fieldName + "\"";
    size_t pos = json.find(search);
    if (pos == std::string::npos) return "";

    size_t colon = json.find(':', pos + search.size());
    if (colon == std::string::npos) return "";

    size_t quoteStart = json.find('"', colon + 1);
    if (quoteStart == std::string::npos) return "";

    size_t quoteEnd = quoteStart + 1;
    while (quoteEnd < json.size()) {
        if (json[quoteEnd] == '"' && json[quoteEnd - 1] != '\\') {
            break;
        }
        quoteEnd++;
    }
    if (quoteEnd >= json.size()) return "";

    return json.substr(quoteStart + 1, quoteEnd - quoteStart - 1);
}

static int extractIntField(const std::string& json, const std::string& fieldName) {
    std::string search = "\"" + fieldName + "\"";
    size_t pos = json.find(search);
    if (pos == std::string::npos) return -1;

    size_t colon = json.find(':', pos + search.size());
    if (colon == std::string::npos) return -1;

    size_t numStart = json.find_first_of("0123456789-", colon + 1);
    if (numStart == std::string::npos) return -1;

    size_t numEnd = json.find_first_not_of("0123456789-", numStart);
    std::string numStr = (numEnd == std::string::npos) ? json.substr(numStart) : json.substr(numStart, numEnd - numStart);

    try {
        return std::stoi(numStr);
    } catch (...) {
        return -1;
    }
}

LspServer::LspServer(std::istream& in, std::ostream& out)
    : inStream(in), outStream(out) {
    rules::registerBuiltinRules();
}

std::string LspServer::uriToPath(const std::string& uri) {
    std::string path = uri;
    if (path.rfind("file://", 0) == 0) {
        path = path.substr(7);
    }
    // Decode percent encodings (e.g., %20 -> space)
    std::string decoded;
    for (size_t i = 0; i < path.size(); ++i) {
        if (path[i] == '%' && i + 2 < path.size()) {
            int val = 0;
            std::istringstream hexStream(path.substr(i + 1, 2));
            if (hexStream >> std::hex >> val) {
                decoded += static_cast<char>(val);
                i += 2;
                continue;
            }
        }
        decoded += path[i];
    }
    return decoded;
}

void LspServer::sendMessage(const std::string& payload) {
    outStream << "Content-Length: " << payload.size() << "\r\n\r\n" << payload;
    outStream.flush();
}

int LspServer::run() {
    std::string headerLine;
    while (!isShutdown && std::getline(inStream, headerLine)) {
        if (headerLine.rfind("Content-Length:", 0) == 0) {
            size_t len = std::stoul(headerLine.substr(15));
            // Read until empty line delimiter
            std::string emptyLine;
            while (std::getline(inStream, emptyLine) && emptyLine != "\r" && !emptyLine.empty()) {
            }

            std::vector<char> buffer(len);
            inStream.read(buffer.data(), len);
            std::string message(buffer.data(), len);

            std::string response = processMessage(message);
            if (!response.empty()) {
                sendMessage(response);
            }
        }
    }
    return 0;
}

std::string LspServer::processMessage(const std::string& rawJson) {
    std::string method = extractStringField(rawJson, "method");
    int id = extractIntField(rawJson, "id");

    if (method == "initialize") {
        std::ostringstream ss;
        ss << "{"
           << "\"jsonrpc\":\"2.0\","
           << "\"id\":" << id << ","
           << "\"result\":{"
           << "  \"capabilities\":{"
           << "    \"textDocumentSync\":1,"
           << "    \"codeActionProvider\":true"
           << "  },"
           << "  \"serverInfo\":{"
           << "    \"name\":\"uvmcheck-lsp\","
           << "    \"version\":\"0.1.0\""
           << "  }"
           << "}}";
        return ss.str();
    }

    if (method == "initialized") {
        return ""; // Notification, no reply
    }

    if (method == "textDocument/didOpen") {
        std::string uri = extractStringField(rawJson, "uri");
        std::string text = extractStringField(rawJson, "text");
        openDocuments[uri] = text;
        auto violations = lintDocument(uri, text);
        publishDiagnostics(uri, violations);
        return "";
    }

    if (method == "textDocument/didChange") {
        std::string uri = extractStringField(rawJson, "uri");
        std::string text = extractStringField(rawJson, "text");
        if (!text.empty()) {
            openDocuments[uri] = text;
        }
        auto violations = lintDocument(uri, openDocuments[uri]);
        publishDiagnostics(uri, violations);
        return "";
    }

    if (method == "textDocument/didSave") {
        std::string uri = extractStringField(rawJson, "uri");
        auto violations = lintDocument(uri, openDocuments[uri]);
        publishDiagnostics(uri, violations);
        return "";
    }

    if (method == "textDocument/didClose") {
        std::string uri = extractStringField(rawJson, "uri");
        openDocuments.erase(uri);
        publishDiagnostics(uri, {});
        return "";
    }

    if (method == "textDocument/codeAction") {
        std::string uri = extractStringField(rawJson, "uri");
        std::ostringstream ss;
        ss << "{"
           << "\"jsonrpc\":\"2.0\","
           << "\"id\":" << id << ","
           << "\"result\":[]"
           << "}";
        return ss.str();
    }

    if (method == "shutdown") {
        isShutdown = true;
        std::ostringstream ss;
        ss << "{\"jsonrpc\":\"2.0\",\"id\":" << id << ",\"result\":null}";
        return ss.str();
    }

    if (method == "exit") {
        isShutdown = true;
        return "";
    }

    if (id >= 0) {
        // Unknown request, return empty result
        std::ostringstream ss;
        ss << "{\"jsonrpc\":\"2.0\",\"id\":" << id << ",\"result\":null}";
        return ss.str();
    }

    return "";
}

std::vector<core::Violation> LspServer::lintDocument(const std::string& uri, const std::string& content) {
    std::string filePath = uriToPath(uri);
    std::string sourceText = content;

    if (sourceText.empty()) {
        std::ifstream file(filePath);
        if (file.is_open()) {
            std::stringstream buffer;
            buffer << file.rdbuf();
            sourceText = buffer.str();
        }
    }

    if (sourceText.empty()) {
        return {};
    }

    slang::SourceManager sm;
    auto tree = slang::syntax::SyntaxTree::fromText(sourceText, sm, "source", filePath);
    slang::ast::Compilation compilation;
    compilation.addSyntaxTree(tree);

    uvm::UvmModel uvmModel(compilation);
    core::DiagnosticEngine diags(sm);
    const auto& projectConfig = core::ConfigManager::instance().getProjectConfig();
    diags.setProjectConfig(&projectConfig);

    // Same default as the CLI: opt-in Advisory rules stay off in the editor.
    const auto advisory = static_cast<uint32_t>(rules::RuleProfile::Advisory);
    std::vector<rules::Rule*> rules;
    for (const auto& rule : rules::RuleRegistry::instance().getRules()) {
        if ((rule->getProfiles() & ~advisory) == 0) continue;
        if (!projectConfig.isRuleEnabled(rule->getId(), rule->getName())) continue;
        rules.push_back(rule.get());
    }
    engine::RuleEngine ruleEngine(compilation, uvmModel, sm, engine::EngineOptions{});
    ruleEngine.run(rules, diags);

    return diags.getViolations();
}

void LspServer::publishDiagnostics(const std::string& uri, const std::vector<core::Violation>& violations) {
    std::ostringstream ss;
    ss << "{"
       << "\"jsonrpc\":\"2.0\","
       << "\"method\":\"textDocument/publishDiagnostics\","
       << "\"params\":{"
       << "  \"uri\":\"" << escapeJson(uri) << "\","
       << "  \"diagnostics\":[";

    for (size_t i = 0; i < violations.size(); ++i) {
        const auto& v = violations[i];
        int severity = 2; // Warning
        if (v.severity == core::Severity::Error) severity = 1;
        else if (v.severity == core::Severity::Info) severity = 3;

        int line = (v.line > 0) ? v.line - 1 : 0;
        int col = (v.column > 0) ? v.column - 1 : 0;

        ss << "{"
           << "  \"range\":{"
           << "    \"start\":{\"line\":" << line << ",\"character\":" << col << "},"
           << "    \"end\":{\"line\":" << line << ",\"character\":" << (col + 10) << "}"
           << "  },"
           << "  \"severity\":" << severity << ","
           << "  \"code\":\"" << escapeJson(v.ruleId) << "\","
           << "  \"source\":\"uvmcheck\","
           << "  \"message\":\"" << escapeJson(v.message) << "\""
           << "}";
        if (i + 1 < violations.size()) ss << ",";
    }

    ss << "]}}";
    sendMessage(ss.str());
}

} // namespace uvmcheck::lsp
