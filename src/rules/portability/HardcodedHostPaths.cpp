#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "slang/ast/ASTVisitor.h"
#include "slang/ast/expressions/CallExpression.h"
#include "slang/ast/expressions/LiteralExpressions.h"
#include "slang/ast/expressions/ConversionExpression.h"

namespace uvmcheck::rules::portability {

class HardcodedPathsVisitor : public engine::AstVisitor<HardcodedPathsVisitor> {
public:
    explicit HardcodedPathsVisitor(core::DiagnosticEngine& diags) : diags(diags) {}

    void handle(const slang::ast::CallExpression& call) {
        std::string subName = std::string(call.getSubroutineName());
        if (subName.starts_with('$')) subName = subName.substr(1);
        static const std::unordered_set<std::string> fileTasks = {
            "readmemh", "readmemb", "writememh", "writememb", "fopen"
        };

        if (fileTasks.count(subName) > 0 && !call.arguments().empty()) {
            const auto* arg = call.arguments()[0];
            while (arg->kind == slang::ast::ExpressionKind::Conversion) {
                arg = &arg->as<slang::ast::ConversionExpression>().operand();
            }

            if (arg->kind == slang::ast::ExpressionKind::StringLiteral) {
                std::string path = std::string(arg->as<slang::ast::StringLiteral>().getValue());

                bool isAbsoluteUnix = path.starts_with('/');
                bool isAbsoluteWin = (path.size() >= 3 && std::isalpha(static_cast<unsigned char>(path[0])) &&
                                      path[1] == ':' && (path[2] == '\\' || path[2] == '/'));

                if (isAbsoluteUnix || isAbsoluteWin) {
                    diags.addViolation(
                        "PORT-PTH-001",
                        "HardcodedHostPaths",
                        "Hardcoded absolute host filesystem path '" + path + "' in system task '$" + subName + "'. "
                        "Absolute paths break regression portability across Linux CI/CD containers and regression clouds. "
                        "Use relative paths or parameterize the path via '$value$plusargs(\"path=%s\", ...)'.",
                        core::Severity::Error,
                        arg->sourceRange.start()
                    );
                }
            }
        }

        visitDefault(call);
    }

private:
    core::DiagnosticEngine& diags;
};

class HardcodedHostPaths : public Rule {
public:
    std::string_view getId() const override { return "PORT-PTH-001"; }
    std::string_view getName() const override { return "HardcodedHostPaths"; }
    std::string_view getDescription() const override {
        return "Disallow hardcoded absolute filesystem paths (/home/..., C:\\...) in file operations ($readmemh, $fopen). "
               "Absolute paths break regression execution in containerized CI/CD pipelines.";
    }
    std::string_view getCategory() const override { return "Portability"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Error; }

    engine::RuleTask check(engine::RuleContext& ctx) override {
        auto& diags = ctx.diags();
        HardcodedPathsVisitor visitor(diags);
        co_await ctx.visitAst(visitor);
    }
};

} // namespace uvmcheck::rules::portability

namespace uvmcheck::rules {
std::unique_ptr<Rule> createHardcodedHostPaths() {
    return std::make_unique<portability::HardcodedHostPaths>();
}
}
