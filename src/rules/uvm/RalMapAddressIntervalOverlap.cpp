#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "uvmcheck/uvm/UvmModel.h"
#include "slang/ast/ASTVisitor.h"
#include "slang/ast/symbols/ClassSymbols.h"
#include "slang/ast/symbols/SubroutineSymbols.h"
#include "slang/ast/expressions/CallExpression.h"
#include "slang/ast/expressions/LiteralExpressions.h"
#include "slang/syntax/AllSyntax.h"
#include <vector>
#include <string>
#include <algorithm>

namespace uvmcheck::rules::uvm {

struct RegMapEntry {
    std::string regName;
    uint64_t offset;
    uint32_t sizeBytes;
    slang::SourceLocation location;
};

class RalMapAddressIntervalOverlap : public Rule {
public:
    std::string_view getId() const override { return "RAL-MAP-002"; }
    std::string_view getName() const override { return "RalMapAddressIntervalOverlap"; }
    std::string_view getDescription() const override {
        return "Detects 1D address interval overlaps and register aliasing collisions within the same "
               "uvm_reg_map in a uvm_reg_block, preventing silent register corruption in hardware.";
    }
    std::string_view getCategory() const override { return "UVM"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Error; }

    void run(slang::ast::Compilation& compilation,
             const uvmcheck::uvm::UvmModel& uvmModel,
             core::DiagnosticEngine& diags) override {

        for (const auto* cls : uvmModel.getAllClasses()) {
            if (!cls) continue;
            std::string clsName(cls->name);
            bool isRegBlock = (clsName.find("reg_block") != std::string::npos ||
                               uvmModel.isDerivedFrom(*cls, "uvm_reg_block"));
            if (!isRegBlock) continue;

            // Map of regName -> declared byte width (default 4 bytes)
            std::unordered_map<std::string, uint32_t> regSizes;
            for (const auto& member : cls->members()) {
                if (member.kind == slang::ast::SymbolKind::ClassProperty) {
                    std::string propName(member.name);
                    std::string typeStr(member.as<slang::ast::ClassPropertySymbol>().getType().toString());
                    uint32_t bytes = 4;
                    if (typeStr.find("64") != std::string::npos || propName.find("64") != std::string::npos) {
                        bytes = 8;
                    } else if (typeStr.find("16") != std::string::npos) {
                        bytes = 2;
                    } else if (typeStr.find("8") != std::string::npos) {
                        bytes = 1;
                    }
                    regSizes[propName] = bytes;
                }
            }

            std::vector<RegMapEntry> entries;

            class AddRegVisitor : public slang::ast::ASTVisitor<AddRegVisitor, slang::ast::VisitFlags::AllGood> {
            public:
                const std::unordered_map<std::string, uint32_t>& sizes;
                std::vector<RegMapEntry>& entries;

                AddRegVisitor(const std::unordered_map<std::string, uint32_t>& s, std::vector<RegMapEntry>& e)
                    : sizes(s), entries(e) {}

                void handle(const slang::ast::CallExpression& call) {
                    if (call.getSubroutineName() == "add_reg") {
                        auto args = call.arguments();
                        if (args.size() >= 2 && args[0] && args[1]) {
                            // Unwrap arg 0 (register expression)
                            const slang::ast::Expression* regExpr = args[0];
                            while (regExpr->kind == slang::ast::ExpressionKind::Conversion) {
                                regExpr = &regExpr->as<slang::ast::ConversionExpression>().operand();
                            }

                            std::string regName;
                            if (regExpr->kind == slang::ast::ExpressionKind::NamedValue) {
                                regName = std::string(regExpr->as<slang::ast::NamedValueExpression>().symbol.name);
                            } else if (regExpr->syntax) {
                                regName = std::string(regExpr->syntax->toString());
                                size_t dot = regName.rfind('.');
                                if (dot != std::string::npos) regName = regName.substr(dot + 1);
                                size_t first = regName.find_first_not_of(" \t\n\r");
                                size_t last = regName.find_last_not_of(" \t\n\r");
                                if (first != std::string::npos) regName = regName.substr(first, last - first + 1);
                            }

                            // Unwrap arg 1 (offset expression)
                            const slang::ast::Expression* offExpr = args[1];
                            while (offExpr->kind == slang::ast::ExpressionKind::Conversion) {
                                offExpr = &offExpr->as<slang::ast::ConversionExpression>().operand();
                            }

                            uint64_t offsetVal = 0;
                            bool validOffset = false;

                            if (offExpr->kind == slang::ast::ExpressionKind::IntegerLiteral) {
                                const auto& lit = offExpr->as<slang::ast::IntegerLiteral>();
                                if (!lit.getValue().hasUnknown()) {
                                    auto v = lit.getValue().as<uint64_t>();
                                    if (v) {
                                        offsetVal = *v;
                                        validOffset = true;
                                    }
                                }
                            }
                            if (!validOffset && offExpr->syntax) {
                                std::string offStr(offExpr->syntax->toString());
                                try {
                                    size_t hPos = offStr.find("'h");
                                    if (hPos != std::string::npos) {
                                        offsetVal = std::stoull(offStr.substr(hPos + 2), nullptr, 16);
                                        validOffset = true;
                                    } else {
                                        offsetVal = std::stoull(offStr, nullptr, 0);
                                        validOffset = true;
                                    }
                                } catch (...) {}
                            }

                            if (validOffset) {
                                uint32_t bSize = 4;
                                auto it = sizes.find(regName);
                                if (it != sizes.end()) {
                                    bSize = it->second;
                                } else {
                                    if (regName.find("64") != std::string::npos) bSize = 8;
                                    else if (regName.find("16") != std::string::npos) bSize = 2;
                                    else if (regName.find("8") != std::string::npos) bSize = 1;
                                }

                                entries.push_back({regName, offsetVal, bSize, call.sourceRange.start()});
                            }
                        }
                    }
                    visitDefault(call);
                }
            };

            AddRegVisitor visitor(regSizes, entries);
            for (const auto& member : cls->members()) {
                if (member.kind == slang::ast::SymbolKind::Subroutine) {
                    member.as<slang::ast::SubroutineSymbol>().getBody().visit(visitor);
                }
            }

            // Multi-pass interval collision check
            for (size_t i = 0; i < entries.size(); ++i) {
                for (size_t j = i + 1; j < entries.size(); ++j) {
                    const auto& e1 = entries[i];
                    const auto& e2 = entries[j];

                    uint64_t start1 = e1.offset;
                    uint64_t end1 = e1.offset + e1.sizeBytes - 1;
                    uint64_t start2 = e2.offset;
                    uint64_t end2 = e2.offset + e2.sizeBytes - 1;

                    if (start1 <= end2 && start2 <= end1) {
                        diags.addViolation(
                            "RAL-MAP-002",
                            "RalMapAddressIntervalOverlap",
                            "Address interval overlap detected in uvm_reg_map: register '" +
                            e2.regName + "' ['h" + std::to_string(start2) + "..'h" + std::to_string(end2) +
                            "] collides with previously mapped register '" +
                            e1.regName + "' ['h" + std::to_string(start1) + "..'h" + std::to_string(end1) +
                            "]. Register address aliasing will cause hardware overwrite corruption.",
                            core::Severity::Error,
                            e2.location
                        );
                    }
                }
            }
        }
    }
};

} // namespace uvmcheck::rules::uvm

namespace uvmcheck::rules {
std::unique_ptr<Rule> createRalMapAddressIntervalOverlap() {
    return std::make_unique<uvm::RalMapAddressIntervalOverlap>();
}
}
