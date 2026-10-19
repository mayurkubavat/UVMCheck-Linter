#pragma once

#include <string>
#include <vector>
#include "slang/ast/symbols/SubroutineSymbols.h"
#include "slang/ast/Statement.h"
#include "uvmcheck/core/Diagnostic.h"

namespace uvmcheck::uvm {

enum class ObjectionState {
    Unheld,
    Held,
    Unknown
};

inline const char* objectionStateToString(ObjectionState state) {
    switch (state) {
        case ObjectionState::Unheld: return "Unheld";
        case ObjectionState::Held: return "Held";
        case ObjectionState::Unknown: return "Unknown";
    }
    return "Unknown";
}

struct FlowViolation {
    std::string ruleId;
    std::string ruleName;
    std::string message;
    core::Severity severity;
    slang::SourceLocation location;
};

class ObjectionControlFlowAnalyzer {
public:
    static std::vector<FlowViolation> analyzeSubroutine(
        const slang::ast::SubroutineSymbol& sub,
        std::string_view className);

private:
    static bool evaluateStatement(
        const slang::ast::Statement& stmt,
        ObjectionState inState,
        ObjectionState& outState,
        std::vector<FlowViolation>& violations,
        const slang::ast::SubroutineSymbol& sub,
        std::string_view className);
};

} // namespace uvmcheck::uvm
