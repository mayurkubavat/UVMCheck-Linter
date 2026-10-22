#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "slang/syntax/SyntaxVisitor.h"
#include "slang/syntax/SyntaxTree.h"
#include "slang/syntax/AllSyntax.h"

namespace uvmcheck::rules::deprecated {

class LegacyRegWireVisitor : public engine::SyntaxVisitor<LegacyRegWireVisitor> {
public:
    explicit LegacyRegWireVisitor(core::DiagnosticEngine& diags) : diags(diags) {}

    // Check 'reg' in data declarations
    void handle(const slang::syntax::DataDeclarationSyntax& decl) {
        if (decl.type->kind == slang::syntax::SyntaxKind::RegType) {
            diags.addViolation(
                "SV-BAN-001",
                "LegacyRegWireInTestbench",
                "Legacy Verilog-95 keyword 'reg' is discouraged in modern SystemVerilog testbenches. "
                "Use 'logic' for unified 4-state storage across procedural and continuous assignments.",
                core::Severity::Info,
                decl.type->getFirstToken().location()
            );
        }
        visitDefault(decl);
    }

    // Check 'wire' in net declarations
    void handle(const slang::syntax::NetDeclarationSyntax& decl) {
        if (decl.netType.kind == slang::parsing::TokenKind::WireKeyword) {
            diags.addViolation(
                "SV-BAN-001",
                "LegacyRegWireInTestbench",
                "Keyword 'wire' is discouraged in SystemVerilog verification code. "
                "In testbenches and interfaces, use 'logic' or 'virtual interface' instead.",
                core::Severity::Info,
                decl.netType.location()
            );
        }
        visitDefault(decl);
    }

private:
    core::DiagnosticEngine& diags;
};

class LegacyRegWireInTestbench : public Rule {
public:
    std::string_view getId() const override { return "SV-BAN-001"; }
    std::string_view getName() const override { return "LegacyRegWireInTestbench"; }
    std::string_view getDescription() const override {
        return "Discourages legacy 'reg' and 'wire' keywords in SystemVerilog testbenches in favor of 'logic'.";
    }
    std::string_view getCategory() const override { return "AntiPattern"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Info; }

    engine::RuleTask check(engine::RuleContext& ctx) override {
        auto& diags = ctx.diags();
        LegacyRegWireVisitor visitor(diags);
        co_await ctx.visitSyntax(visitor);
    }
};

} // namespace uvmcheck::rules::deprecated

namespace uvmcheck::rules {
std::unique_ptr<Rule> createLegacyRegWireInTestbench() {
    return std::make_unique<deprecated::LegacyRegWireInTestbench>();
}
}
