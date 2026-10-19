#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "slang/ast/ASTVisitor.h"
#include "slang/ast/types/AllTypes.h"

namespace uvmcheck::rules::style {

class TypedefVisitor : public slang::ast::ASTVisitor<TypedefVisitor, slang::ast::VisitFlags::AllGood> {
public:
    explicit TypedefVisitor(core::DiagnosticEngine& diags) : diags(diags) {}

    void handle(const slang::ast::TypeAliasType& alias) {
        if (!alias.name.empty()) {
            const auto& underlying = alias.targetType.getType();
            if (underlying.isEnum()) {
                if (!alias.name.ends_with("_e")) {
                    diags.addViolation(
                        "SV-STY-004",
                        "TypedefNameSuffix",
                        "Enumeration typedef '" + std::string(alias.name) +
                        "' does not end with standard suffix '_e'. By convention, enum types must end with '_e'.",
                        core::Severity::Warning,
                        alias.location
                    );
                }
            } else if (underlying.isStruct()) {
                if (!alias.name.ends_with("_t")) {
                    diags.addViolation(
                        "SV-STY-004",
                        "TypedefNameSuffix",
                        "Structure typedef '" + std::string(alias.name) +
                        "' does not end with standard suffix '_t'. By convention, struct and data types must end with '_t'.",
                        core::Severity::Warning,
                        alias.location
                    );
                }
            }
        }
        visitDefault(alias);
    }

private:
    core::DiagnosticEngine& diags;
};

class TypedefNameSuffix : public Rule {
public:
    std::string_view getId() const override { return "SV-STY-004"; }
    std::string_view getName() const override { return "TypedefNameSuffix"; }
    std::string_view getDescription() const override {
        return "Enforces naming conventions for typedefs: enums must end with '_e', "
               "and structs/data types must end with '_t'.";
    }
    std::string_view getCategory() const override { return "Style"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Warning; }

    void run(slang::ast::Compilation& compilation,
             const uvmcheck::uvm::UvmModel&,
             core::DiagnosticEngine& diags) override {
        TypedefVisitor visitor(diags);
        compilation.getRoot().visit(visitor);
    }
};

} // namespace uvmcheck::rules::style

namespace uvmcheck::rules {
std::unique_ptr<Rule> createTypedefNameSuffix() {
    return std::make_unique<style::TypedefNameSuffix>();
}
}
