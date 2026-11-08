#pragma once

#include <iostream>
#include <map>
#include <string>
#include <vector>

namespace uvmcheck::lsp {

/// A text edit in LSP coordinates (0-based lines and characters).
struct LspEdit {
    int startLine = 0, startChar = 0, endLine = 0, endChar = 0;
    std::string newText;
};

struct LspFix {
    std::string title;
    LspEdit edit;
    /// Safe fixes are the preferred quick fix. Unsafe and display-only fixes
    /// are offered, but not preferred.
    bool preferred = true;
};

struct LspRelated {
    std::string uri;
    int line = 0, character = 0;
    std::string message;
};

struct LspDiagnostic {
    int line = 0, startChar = 0, endChar = 0; // 0-based, single-line range
    int severity = 2;                         // 1 error, 2 warning, 3 information
    std::string code, ruleName, message;
    std::vector<LspFix> fixes;
    std::vector<LspRelated> related;
};

/// Language server. Each lint compiles the project the way the CLI does
/// (sources, file lists, include dirs and defines from the project config),
/// with the editor's unsaved text standing in for the files on disk, and
/// publishes findings for every open document. Without project sources, the
/// open documents are compiled together on their own.
class LspServer {
public:
    LspServer(std::istream& in = std::cin, std::ostream& out = std::cout);

    /// Runs the main LSP JSON-RPC loop until exit notification
    int run();

    /// Processes a single JSON-RPC message payload and returns the response
    /// (empty for notifications). Diagnostics are sent as they are produced.
    std::string processMessage(const std::string& rawJson);

    /// Lints all open documents; returns findings keyed by document URI.
    std::map<std::string, std::vector<LspDiagnostic>> lintOpenDocuments();

private:
    std::istream& inStream;
    std::ostream& outStream;
    bool isShutdown = false;
    std::string rootPath;
    std::map<std::string, std::string> openDocuments; // uri -> text
    std::map<std::string, std::vector<LspDiagnostic>> lastDiagnostics;

    void sendMessage(const std::string& payload);
    void lintAndPublish();
    void publishDiagnostics(const std::string& uri, const std::vector<LspDiagnostic>& diags);
    std::string codeActions(const std::string& uri, int firstLine, int lastLine) const;
    void loadProjectConfig();

public:
    static std::string uriToPath(const std::string& uri);
    static std::string pathToUri(const std::string& path);
};

} // namespace uvmcheck::lsp
