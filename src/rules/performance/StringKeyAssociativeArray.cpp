#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "slang/ast/ASTVisitor.h"
#include "slang/ast/symbols/VariableSymbols.h"
#include "slang/ast/types/AllTypes.h"

namespace uvmcheck::rules::performance {

class StringKeyAssocArrayVisitor : public slang::ast::ASTVisitor<StringKeyAssocArrayVisitor, slang::ast::VisitFlags::Symbols> {
public:
    explicit StringKeyAssocArrayVisitor(core::DiagnosticEngine& diags) : diags(diags) {}

    void handle(const slang::ast::VariableSymbol& var) {
        const auto& type = var.getType();
        if (const auto* assoc = type.as_if<slang::ast::AssociativeArrayType>()) {
            if (assoc->indexType && assoc->indexType->isString()) {
                diags.addViolation(
                    "SV-PRF-006",
                    "StringKeyAssociativeArray",
                    "Associative array '" + std::string(var.name) +
                    "' is indexed by 'string'. Associative string lookups incur dynamic hashing and heap memory allocations on every element access, severely degrading simulation performance in hot transaction paths. Use an integral type, enum, or integer ID instead.",
                    core::Severity::Warning,
                    var.location
                );
            }
        }
        visitDefault(var);
    }

private:
    core::DiagnosticEngine& diags;
};

class StringKeyAssociativeArray : public Rule {
public:
    std::string_view getId() const override { return "SV-PRF-006"; }
    std::string_view getName() const override { return "StringKeyAssociativeArray"; }
    std::string_view getDescription() const override {
        return "Detects associative arrays indexed by 'string'. String keys incur hash calculation and dynamic "
               "string comparisons on every lookup and insert, degrading simulation throughput.";
    }
    std::string_view getCategory() const override { return "Performance"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Warning; }

    void run(slang::ast::Compilation& compilation,
             const uvmcheck::uvm::UvmModel& /*uvmModel*/,
             core::DiagnosticEngine& diags) override {
        StringKeyAssocArrayVisitor visitor(diags);
        compilation.getRoot().visit(visitor);
    }
};

} // namespace uvmcheck::rules::performance

namespace uvmcheck::rules {
std::unique_ptr<Rule> createStringKeyAssociativeArray() {
    return std::make_unique<performance::StringKeyAssociativeArray>();
}
}
