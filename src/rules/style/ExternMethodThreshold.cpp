#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "slang/syntax/SyntaxVisitor.h"
#include "slang/syntax/SyntaxTree.h"
#include "slang/syntax/AllSyntax.h"

namespace uvmcheck::rules::style {

class ExternMethodVisitor : public slang::syntax::SyntaxVisitor<ExternMethodVisitor> {
public:
    ExternMethodVisitor(const slang::SourceManager& sm, core::DiagnosticEngine& diags, size_t threshold = 5)
        : sm(sm), diags(diags), threshold(threshold) {}

    void handle(const slang::syntax::ClassMethodDeclarationSyntax& method) {
        // If declared with 'extern' keyword, it is already an external prototype
        for (const auto& qual : method.qualifiers) {
            if (qual.kind == slang::parsing::TokenKind::ExternKeyword) {
                visitDefault(method);
                return;
            }
        }

        const auto& fn = *method.declaration;
        std::string routineName = fn.prototype->name->toString();
        routineName.erase(std::remove_if(routineName.begin(), routineName.end(), [](unsigned char c) { return std::isspace(c); }), routineName.end());

        // Skip trivial constructors if they just call super.new
        size_t startLine = sm.getLineNumber(fn.getFirstToken().location());
        size_t endLine = sm.getLineNumber(fn.getLastToken().location());
        size_t lineCount = (endLine >= startLine) ? (endLine - startLine + 1) : 1;

        if (lineCount > threshold) {
            diags.addViolation(
                "SV-STY-009",
                "ExternMethodThreshold",
                "Method '" + routineName + "' implemented inline inside class spans " +
                std::to_string(lineCount) + " lines (threshold: " + std::to_string(threshold) +
                " lines). In common SystemVerilog style guidelines, large method implementations should be moved outside the class declaration using 'extern' to maintain a clean class API.",
                core::Severity::Warning,
                fn.getFirstToken().location()
            );
        }

        visitDefault(method);
    }

private:
    const slang::SourceManager& sm;
    core::DiagnosticEngine& diags;
    size_t threshold;
};

class ExternMethodThreshold : public Rule {
public:
    std::string_view getId() const override { return "SV-STY-009"; }
    std::string_view getName() const override { return "ExternMethodThreshold"; }
    std::string_view getDescription() const override {
        return "Enforces 'extern' declaration for class methods exceeding N lines of code (default: 5 lines). "
               "Keeps class declarations concise and readable as clean interface definitions.";
    }
    std::string_view getCategory() const override { return "Architecture"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Warning; }

    void run(slang::ast::Compilation& compilation,
             const uvmcheck::uvm::UvmModel&,
             core::DiagnosticEngine& diags) override {
        ExternMethodVisitor visitor(*compilation.getSourceManager(), diags, 5);
        for (const auto& tree : compilation.getSyntaxTrees()) {
            tree->root().visit(visitor);
        }
    }
};

} // namespace uvmcheck::rules::style

namespace uvmcheck::rules {
std::unique_ptr<Rule> createExternMethodThreshold() {
    return std::make_unique<style::ExternMethodThreshold>();
}
}
