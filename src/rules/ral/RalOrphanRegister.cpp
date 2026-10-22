#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "slang/syntax/SyntaxVisitor.h"
#include "slang/syntax/AllSyntax.h"
#include "slang/syntax/SyntaxTree.h"

namespace uvmcheck::rules::ral {

class RalOrphanVisitor : public engine::SyntaxVisitor<RalOrphanVisitor> {
public:
    explicit RalOrphanVisitor(core::DiagnosticEngine& diags) : diags(diags) {}

    void handle(const slang::syntax::ClassDeclarationSyntax& cls) {
        std::string clsName = std::string(cls.name.valueText());
        std::string lowerCls = toLower(clsName);

        bool isRegBlock = (lowerCls.find("reg_block") != std::string::npos) ||
                          (lowerCls.find("reg_model") != std::string::npos) ||
                          (cls.extendsClause && cls.extendsClause->toString().find("uvm_reg_block") != std::string::npos);

        if (isRegBlock) {
            for (const auto* member : cls.items) {
                const slang::syntax::FunctionDeclarationSyntax* routine = nullptr;
                if (member->kind == slang::syntax::SyntaxKind::ClassMethodDeclaration) {
                    routine = member->as<slang::syntax::ClassMethodDeclarationSyntax>().declaration;
                } else if (member->kind == slang::syntax::SyntaxKind::FunctionDeclaration) {
                    routine = &member->as<slang::syntax::FunctionDeclarationSyntax>();
                }

                if (routine && routine->prototype) {
                    std::string fnName = std::string(routine->prototype->name->getFirstToken().valueText());
                    if (fnName == "build") {
                        std::string buildBody = stripComments(std::string(routine->toString()));

                        // Find instantiated registers (e.g. REG = ...::create or REG = new)
                        // Scan lines for assignments ending in create or new
                        std::unordered_set<std::string> instantiatedRegs;
                        std::unordered_set<std::string> mappedRegs;

                        // Parse add_reg calls: add_reg(REG, ...)
                        size_t addPos = 0;
                        while ((addPos = buildBody.find(".add_reg", addPos)) != std::string::npos) {
                            size_t openParen = buildBody.find('(', addPos);
                            if (openParen != std::string::npos) {
                                size_t comma = buildBody.find(',', openParen);
                                if (comma != std::string::npos) {
                                    std::string regArg = buildBody.substr(openParen + 1, comma - openParen - 1);
                                    regArg = trim(regArg);
                                    if (!regArg.empty()) {
                                        mappedRegs.insert(regArg);
                                    }
                                }
                            }
                            addPos += 8;
                        }

                        // Search for candidate register assignments in buildBody
                        size_t createPos = 0;
                        while ((createPos = buildBody.find("::type_id::create", createPos)) != std::string::npos) {
                            size_t eqPos = buildBody.rfind('=', createPos);
                            if (eqPos != std::string::npos) {
                                size_t lineStart = buildBody.rfind('\n', eqPos);
                                if (lineStart == std::string::npos) lineStart = 0;
                                else lineStart += 1;

                                std::string lhs = buildBody.substr(lineStart, eqPos - lineStart);
                                lhs = trim(lhs);
                                if (!lhs.empty() && lhs.find(' ') == std::string::npos) {
                                    instantiatedRegs.insert(lhs);
                                }
                            }
                            createPos += 18;
                        }

                        for (const auto& regName : instantiatedRegs) {
                            if (mappedRegs.count(regName) == 0) {
                                diags.addViolation(
                                    "RAL-MAP-001",
                                    "RalOrphanRegister",
                                    "Orphan register '" + regName + "' in register block '" + clsName +
                                    "': instantiated in build() but never mapped via 'default_map.add_reg(" + regName +
                                    ", ...)'. Orphan registers lack an address mapping, causing frontdoor write/read operations to fail.",
                                    core::Severity::Error,
                                    routine->prototype->name->sourceRange().start()
                                );
                            }
                        }
                    }
                }
            }
        }

        visitDefault(cls);
    }

    static std::string stripComments(const std::string& input) {
        std::string result;
        result.reserve(input.size());
        bool inLineComment = false;
        for (size_t i = 0; i < input.size(); ++i) {
            if (!inLineComment && i + 1 < input.size() && input[i] == '/' && input[i+1] == '/') {
                inLineComment = true;
                ++i;
            } else if (inLineComment && input[i] == '\n') {
                inLineComment = false;
                result += '\n';
            } else if (!inLineComment) {
                result += input[i];
            }
        }
        return result;
    }

    static std::string trim(const std::string& str) {
        size_t first = str.find_first_not_of(" \t\r\n");
        if (first == std::string::npos) return "";
        size_t last = str.find_last_not_of(" \t\r\n");
        return str.substr(first, last - first + 1);
    }

    static std::string toLower(std::string s) {
        for (char& c : s) c = static_cast<char>(std::tolower(static_cast<unsigned char>(c)));
        return s;
    }

    core::DiagnosticEngine& diags;
};

class RalOrphanRegister : public Rule {
public:
    std::string_view getId() const override { return "RAL-MAP-001"; }
    std::string_view getName() const override { return "RalOrphanRegister"; }
    std::string_view getDescription() const override {
        return "Detects instantiated uvm_reg instances inside uvm_reg_block that are never mapped via default_map.add_reg(). "
               "Orphan registers cause frontdoor access failures and verify phantom memory.";
    }
    std::string_view getCategory() const override { return "UVM"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Error; }

    engine::RuleTask check(engine::RuleContext& ctx) override {
        auto& diags = ctx.diags();
        RalOrphanVisitor visitor(diags);
        co_await ctx.visitSyntax(visitor);
    }
};

} // namespace uvmcheck::rules::ral

namespace uvmcheck::rules {
std::unique_ptr<Rule> createRalOrphanRegister() {
    return std::make_unique<ral::RalOrphanRegister>();
}
}
