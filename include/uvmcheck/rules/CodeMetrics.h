#pragma once

// Size, complexity, coupling and cohesion metrics shared by the SV-MET and SV-DSN rules.
// Routine metrics are computed on demand; class metrics are computed once per compilation
// and cached, since several rules read them.

#include <string>
#include <unordered_map>
#include <vector>

#include "slang/ast/Compilation.h"
#include "slang/ast/symbols/ClassSymbols.h"
#include "slang/ast/symbols/SubroutineSymbols.h"
#include "slang/text/SourceLocation.h"
#include "uvmcheck/uvm/UvmModel.h"

namespace uvmcheck::rules::metrics {

struct RoutineMetrics {
    int cognitive = 0;          // Sonar cognitive complexity (S3776)
    int cyclomatic = 1;         // McCabe, for WMC
    int maxNesting = 0;         // deepest control-structure nesting (1 = top level)
    slang::SourceLocation deepest;
    int statements = 0;
    int arguments = 0;
    int maxBoolOps = 0;         // most &&/|| operators in one condition
    slang::SourceLocation mostComplexCondition;
};

RoutineMetrics computeRoutineMetrics(const slang::ast::SubroutineSymbol& sub);

struct ClassMetrics {
    const slang::ast::ClassType* cls = nullptr;
    int wmc = 0;                // weighted methods per class (sum of cyclomatic complexity)
    int atfd = 0;               // access to foreign data: distinct foreign properties used
    double tcc = 1.0;           // tight class cohesion: share of method pairs using a common field
    int lcom4 = 1;              // connected groups of methods (shared fields or calls)
    std::vector<std::vector<std::string>> lcomGroups; // method names per group
    int cbo = 0;                // distinct user class types referenced
    std::vector<std::string> coupledTo;
    std::vector<const slang::ast::ClassType*> coupledClasses; // same set, by symbol
    int dit = 0;                // user-defined inheritance levels above the UVM/base class
    std::vector<std::string> ancestry; // user ancestors, nearest first
    int fields = 0;
    int methods = 0;
};

/// Metrics for every user class, computed once per compilation.
const std::vector<ClassMetrics>& classMetrics(slang::ast::Compilation& comp, const uvm::UvmModel& model);

} // namespace uvmcheck::rules::metrics
