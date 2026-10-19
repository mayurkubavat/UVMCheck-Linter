#pragma once

#include <string>
#include <vector>
#include <unordered_map>
#include <iostream>
#include "uvmcheck/core/Diagnostic.h"

namespace uvmcheck::lsp {

class LspServer {
public:
    LspServer(std::istream& in = std::cin, std::ostream& out = std::cout);

    /// Runs the main LSP JSON-RPC loop until exit notification
    int run();

    /// Processes a single JSON-RPC message payload (public for unit testing)
    std::string processMessage(const std::string& rawJson);

    /// Lints a document given its URI and optional in-memory source content
    std::vector<core::Violation> lintDocument(const std::string& uri, const std::string& content);

private:
    std::istream& inStream;
    std::ostream& outStream;
    bool isShutdown = false;
    std::unordered_map<std::string, std::string> openDocuments;

    void sendMessage(const std::string& payload);
    void publishDiagnostics(const std::string& uri, const std::vector<core::Violation>& violations);
    static std::string uriToPath(const std::string& uri);
};

} // namespace uvmcheck::lsp
