#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "slang/syntax/SyntaxVisitor.h"
#include "slang/syntax/AllSyntax.h"
#include "slang/syntax/SyntaxTree.h"

namespace uvmcheck::rules::ral {

class RalCollisionVisitor : public engine::SyntaxVisitor<RalCollisionVisitor> {
public:
    explicit RalCollisionVisitor(core::DiagnosticEngine& diags) : diags(diags) {}

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

                        // Map offset -> reg_name
                        std::unordered_map<std::string, std::string> offsetToReg;

                        size_t addPos = 0;
                        while ((addPos = buildBody.find(".add_reg", addPos)) != std::string::npos) {
                            size_t openParen = buildBody.find('(', addPos);
                            if (openParen != std::string::npos) {
                                size_t comma1 = buildBody.find(',', openParen);
                                if (comma1 != std::string::npos) {
                                    size_t comma2 = buildBody.find(',', comma1 + 1);
                                    size_t closeParen = buildBody.find(')', comma1 + 1);
                                    size_t endOffset = (comma2 != std::string::npos && comma2 < closeParen) ? comma2 : closeParen;

                                    if (endOffset != std::string::npos) {
                                        std::string regArg = trim(buildBody.substr(openParen + 1, comma1 - openParen - 1));
                                        std::string offsetArg = trim(buildBody.substr(comma1 + 1, endOffset - comma1 - 1));

                                        if (!regArg.empty() && !offsetArg.empty()) {
                                            auto it = offsetToReg.find(offsetArg);
                                            if (it != offsetToReg.end()) {
                                                diags.addViolation(
                                                    "RAL-COL-001",
                                                    "RalAddressCollision",
                                                    "Register address collision in register block '" + clsName +
                                                    "': register '" + regArg + "' is mapped to offset " + offsetArg +
                                                    ", which is already occupied by register '" + it->second +
                                                    "'. Address collisions cause silent register overwrites and aliasing bugs.",
                                                    core::Severity::Error,
                                                    routine->prototype->name->sourceRange().start()
                                                );
                                            } else {
                                                offsetToReg[offsetArg] = regArg;
                                            }
                                        }
                                    }
                                }
                            }
                            addPos += 8;
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

class RalAddressCollision : public Rule {
public:
    std::string_view getId() const override { return "RAL-COL-001"; }
    std::string_view getName() const override { return "RalAddressCollision"; }
    std::string_view getDescription() const override {
        return "Detects duplicate/overlapping address offsets mapped in uvm_reg_block address maps. "
               "Address collisions cause silent register overwriting and hardware aliasing bugs.";
    }
    std::string_view getCategory() const override { return "UVM"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Error; }

    engine::RuleTask check(engine::RuleContext& ctx) override {
        auto& diags = ctx.diags();
        RalCollisionVisitor visitor(diags);
        co_await ctx.visitSyntax(visitor);
    }
};

} // namespace uvmcheck::rules::ral

namespace uvmcheck::rules {
std::unique_ptr<Rule> createRalAddressCollision() {
    return std::make_unique<ral::RalAddressCollision>();
}
}
