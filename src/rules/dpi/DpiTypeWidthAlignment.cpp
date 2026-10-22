#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "slang/syntax/SyntaxVisitor.h"
#include "slang/syntax/AllSyntax.h"
#include "slang/syntax/SyntaxTree.h"

namespace uvmcheck::rules::dpi {

class DpiTypeVisitor : public engine::SyntaxVisitor<DpiTypeVisitor> {
public:
    explicit DpiTypeVisitor(core::DiagnosticEngine& diags) : diags(diags) {}

    void handle(const slang::syntax::DPIImportSyntax& dpiImport) {
        if (dpiImport.method) {
            std::string protoText = std::string(dpiImport.method->toString());
            std::string fnName = std::string(dpiImport.method->name->getFirstToken().valueText());

            // 4-state types like integer or reg require complex svLogicVecVal arrays in C
            if (protoText.find("integer") != std::string::npos ||
                protoText.find(" reg ") != std::string::npos ||
                protoText.find("(reg ") != std::string::npos) {
                diags.addViolation(
                    "DPI-TYP-001",
                    "DpiTypeWidthAlignment",
                    "DPI-C function '" + fnName + "' uses 4-state type ('integer'/'reg') across the C boundary. "
                    "In IEEE 1800 § 35.5, 4-state types require complex 'svLogicVecVal' struct layouts in C/C++ FFI. "
                    "Use portable 2-state types ('int' for int32_t, 'longint' for int64_t, 'byte' for int8_t) for direct C99 ABI compatibility.",
                    core::Severity::Warning,
                    dpiImport.method->name->sourceRange().start()
                );
            }
        }
        visitDefault(dpiImport);
    }

private:
    core::DiagnosticEngine& diags;
};

class DpiTypeWidthAlignment : public Rule {
public:
    std::string_view getId() const override { return "DPI-TYP-001"; }
    std::string_view getName() const override { return "DpiTypeWidthAlignment"; }
    std::string_view getDescription() const override {
        return "Disallow 4-state types (integer, reg) in DPI-C declarations. 4-state types require complex "
               "svLogicVecVal structs in C; use 2-state portable types (int, longint, byte) for C99 ABI compatibility.";
    }
    std::string_view getCategory() const override { return "DPI"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Warning; }

    engine::RuleTask check(engine::RuleContext& ctx) override {
        auto& diags = ctx.diags();
        DpiTypeVisitor visitor(diags);
        co_await ctx.visitSyntax(visitor);
    }
};

} // namespace uvmcheck::rules::dpi

namespace uvmcheck::rules {
std::unique_ptr<Rule> createDpiTypeWidthAlignment() {
    return std::make_unique<dpi::DpiTypeWidthAlignment>();
}
}
