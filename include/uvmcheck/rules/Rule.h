#pragma once

#include <string_view>
#include <memory>
#include <vector>
#include "slang/ast/Compilation.h"
#include "uvmcheck/core/Diagnostic.h"
#include "uvmcheck/uvm/UvmModel.h"
#include "uvmcheck/engine/RuleContext.h"
#include "uvmcheck/engine/RuleTask.h"
#include "uvmcheck/rules/Guidelines.h"

namespace uvmcheck::rules {

enum class RuleProfile : uint32_t {
    None        = 0,
    Soundness   = 1 << 0, // Critical verification bugs, race conditions, false passes
    Methodology = 1 << 1, // UVM & OOP architectural hygiene
    Performance = 1 << 2, // Simulation speed optimization
    Style       = 1 << 3, // Naming, banner, formatting, readability
    Security    = 1 << 4, // Secrets in logs, host path leaks, portability
    Smells      = 1 << 5, // Code smells, maintainability, architectural debt
    Advisory    = 1 << 6, // Opinionated checks, off unless selected by --preset or --rule
    All         = 0xFFFFFFFF
};

inline uint32_t operator|(RuleProfile a, RuleProfile b) {
    return static_cast<uint32_t>(a) | static_cast<uint32_t>(b);
}
inline uint32_t operator|(uint32_t a, RuleProfile b) {
    return a | static_cast<uint32_t>(b);
}
inline bool hasProfile(uint32_t mask, RuleProfile profile) {
    return (mask & static_cast<uint32_t>(profile)) != 0;
}

class Rule {
public:
    virtual ~Rule() = default;

    /// Rule unique identifier (e.g. "UVM-FAC-001")
    virtual std::string_view getId() const = 0;

    /// Short descriptive name (e.g. "UvmComponentDirectNew")
    virtual std::string_view getName() const = 0;

    /// Explanation of why this rule exists
    virtual std::string_view getDescription() const = 0;

    /// Category: "UVM", "OOP", "Performance", "Style"
    virtual std::string_view getCategory() const = 0;

    /// Default severity (Error, Warning, Info)
    virtual core::Severity getDefaultSeverity() const = 0;

    /// Methodology guidelines this rule enforces ("MG-4.1" ...); see docs/methodology/.
    virtual std::vector<std::string_view> getGuidelines() const {
        return guidelinesForRule(getId());
    }

    /// Preset profile bitmask (defaults based on category or overridden by rule)
    virtual uint32_t getProfiles() const {
        std::string_view cat = getCategory();
        if (cat == "Concurrency" || cat == "Correctness" || cat == "Randomization" ||
            cat == "Types" || cat == "DeadCode" || cat == "RTL_TB_Bridge" || cat == "SVA") {
            return static_cast<uint32_t>(RuleProfile::Soundness);
        }
        if (cat == "UVM" || cat == "OOP" || cat == "Architecture" || cat == "Deprecated") {
            return static_cast<uint32_t>(RuleProfile::Methodology);
        }
        if (cat == "Performance") {
            return static_cast<uint32_t>(RuleProfile::Performance);
        }
        if (cat == "Style") {
            return static_cast<uint32_t>(RuleProfile::Style);
        }
        if (cat == "Security" || cat == "Portability" || cat == "DPI") {
            return static_cast<uint32_t>(RuleProfile::Security) | static_cast<uint32_t>(RuleProfile::Soundness);
        }
        if (cat == "Advisory") {
            return static_cast<uint32_t>(RuleProfile::Advisory);
        }
        if (cat == "Smell" || cat == "Maintainability") {
            return static_cast<uint32_t>(RuleProfile::Smells);
        }
        return static_cast<uint32_t>(RuleProfile::Soundness);
    }

    /// Entry point used by the rule engine.
    ///
    /// Rules that walk the whole design should override this and hand their
    /// visitors to the shared walk instead of walking themselves:
    ///
    ///     engine::RuleTask check(engine::RuleContext& ctx) override {
    ///         MyVisitor visitor(ctx.diags());
    ///         co_await ctx.visitAst(visitor);   // or ctx.visitSyntax(visitor)
    ///         // ... optional post-walk reporting ...
    ///     }
    ///
    /// The default runs the rule's own run() as an independent parallel job.
    virtual engine::RuleTask check(engine::RuleContext& ctx) {
        co_await ctx.standaloneJob();
        run(ctx.compilation(), ctx.uvmModel(), ctx.diags());
    }

    /// Standalone check against compilation AST and UVM domain model, for rules
    /// that do not use the shared walk (or need a custom traversal).
    virtual void run(slang::ast::Compilation& /*compilation*/,
                     const uvm::UvmModel& /*uvmModel*/,
                     core::DiagnosticEngine& /*diags*/) {}
};

} // namespace uvmcheck::rules
