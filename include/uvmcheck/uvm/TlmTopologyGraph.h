#pragma once

#include <string>
#include <vector>
#include "slang/ast/Compilation.h"
#include "uvmcheck/uvm/UvmModel.h"
#include "uvmcheck/core/Diagnostic.h"

namespace uvmcheck::uvm {

struct TlmPortDecl {
    std::string componentClass;
    std::string portName;
    std::string typeName;
    bool isPullPort;
    bool isAnalysisPort;
    slang::SourceLocation location;
};

struct TlmConnectionRecord {
    std::string callerClass;
    std::string sourceExpr;
    std::string targetExpr;
    slang::SourceLocation location;
};

class TlmTopologyGraph {
public:
    TlmTopologyGraph(slang::ast::Compilation& compilation, const UvmModel& uvmModel);

    const std::vector<TlmPortDecl>& getPorts() const { return declaredPorts; }
    const std::vector<TlmConnectionRecord>& getConnections() const { return connections; }

    /// Returns list of declared ports that are never connected across any connect_phase
    std::vector<TlmPortDecl> findUnconnectedPorts() const;

private:
    std::vector<TlmPortDecl> declaredPorts;
    std::vector<TlmConnectionRecord> connections;

    void discoverPorts(const UvmModel& uvmModel);
    void discoverConnections(slang::ast::Compilation& compilation);
};

} // namespace uvmcheck::uvm
