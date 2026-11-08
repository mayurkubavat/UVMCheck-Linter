#pragma once

#include <string>
#include <vector>
#include "slang/ast/Compilation.h"
#include "uvmcheck/core/Diagnostic.h"

namespace uvmcheck::uvm {

struct ConfigDbNode {
    enum class Kind { Set, Get };
    Kind kind;
    std::string typeName;
    std::string contextClass;
    std::string instName;
    std::string fieldName;
    slang::SourceLocation location;
};

struct ConfigDbMismatch {
    std::string fieldName;
    std::string setClass;
    std::string setType;
    std::string getClass;
    std::string getType;
    slang::SourceLocation getLocation;
    slang::SourceLocation setLocation;
};

class ConfigDbGraph {
public:
    explicit ConfigDbGraph(slang::ast::Compilation& compilation);

    const std::vector<ConfigDbNode>& getNodes() const { return nodes; }

    /// Finds cross-component type mismatches between set() and get() calls
    std::vector<ConfigDbMismatch> findMismatches() const;

private:
    std::vector<ConfigDbNode> nodes;

    void collectNodes(slang::ast::Compilation& compilation);
};

} // namespace uvmcheck::uvm
