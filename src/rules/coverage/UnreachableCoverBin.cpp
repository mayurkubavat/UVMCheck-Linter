// SV-COV-017: a coverpoint bin whose values the coverpoint expression can never take.

#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"

#include "slang/ast/ASTVisitor.h"
#include "slang/ast/EvalContext.h"
#include "slang/ast/expressions/ConversionExpression.h"
#include "slang/ast/expressions/OperatorExpressions.h"
#include "slang/ast/symbols/CoverSymbols.h"

#include <limits>
#include <optional>

namespace uvmcheck::rules::coverage {

using namespace slang::ast;

namespace {

struct Range {
    long double lo, hi;
};

std::optional<long double> constantOf(const Expression& expr, EvalContext& ec) {
    // Bin values are converted to the coverpoint's type; judge the value as written.
    const Expression* p = &expr;
    while (p->kind == ExpressionKind::Conversion) p = &p->as<ConversionExpression>().operand();
    const Expression& e = *p;
    if (e.kind == ExpressionKind::UnboundedLiteral) return std::nullopt;
    slang::ConstantValue cv = e.getConstant() ? *e.getConstant() : e.eval(ec);
    if (!cv.isInteger() || cv.integer().hasUnknown()) return std::nullopt;
    auto v = cv.integer().as<int64_t>();
    if (!v) return std::nullopt;
    return static_cast<long double>(*v);
}

class BinVisitor : public engine::AstVisitor<BinVisitor> {
public:
    explicit BinVisitor(core::DiagnosticEngine& diags) : diags(diags) {}

    void handle(const CoverpointSymbol& cp) {
        const Type& t = cp.getType();
        if (t.isIntegral()) check(cp, t);
        visitDefault(cp);
    }

private:
    void check(const CoverpointSymbol& cp, const Type& t) {
        const auto width = t.getBitWidth();
        if (width == 0 || width > 62) return;
        const long double span = static_cast<long double>(1ULL << width);
        const Range domain = t.isSigned() ? Range{-span / 2, span / 2 - 1} : Range{0, span - 1};

        EvalContext ec(ASTContext(cp, LookupLocation::max));
        for (const auto& member : cp.members()) {
            if (member.kind != SymbolKind::CoverageBin) continue;
            const auto& bin = member.as<CoverageBinSymbol>();
            if (bin.isDefault || bin.isDefaultSequence || bin.binsKind != CoverageBinSymbol::Bins) continue;
            auto values = bin.getValues();
            if (values.empty()) continue;

            bool reachable = false, known = true;
            for (const auto* v : values) {
                long double lo, hi;
                if (v->kind == ExpressionKind::ValueRange) {
                    const auto& r = v->as<ValueRangeExpression>();
                    if (r.rangeKind != ValueRangeKind::Simple) { known = false; break; }
                    auto l = r.left().kind == ExpressionKind::UnboundedLiteral ? std::optional<long double>(-1e30L)
                                                                                : constantOf(r.left(), ec);
                    auto h = r.right().kind == ExpressionKind::UnboundedLiteral ? std::optional<long double>(1e30L)
                                                                                 : constantOf(r.right(), ec);
                    if (!l || !h) { known = false; break; }
                    lo = std::min(*l, *h);
                    hi = std::max(*l, *h);
                } else {
                    auto c = constantOf(*v, ec);
                    if (!c) { known = false; break; }
                    lo = hi = *c;
                }
                if (hi >= domain.lo && lo <= domain.hi) { reachable = true; break; }
            }
            if (!known || reachable) continue;
            diags.addViolation(
                "SV-COV-017", "UnreachableCoverBin",
                "Bin '" + std::string(bin.name) + "' of coverpoint '" + std::string(cp.name) +
                "' only lists values outside what the " + std::to_string(width) + "-bit " +
                (t.isSigned() ? "signed" : "unsigned") + " coverpoint can hold (" +
                std::to_string(static_cast<long long>(domain.lo)) + " to " +
                std::to_string(static_cast<long long>(domain.hi)) + "). It can never be hit, so coverage "
                "never reaches 100%. Fix the range or widen the coverpoint expression.",
                core::Severity::Warning, bin.location);
        }
    }

    core::DiagnosticEngine& diags;
};

} // namespace

class UnreachableCoverBin : public Rule {
public:
    std::string_view getId() const override { return "SV-COV-017"; }
    std::string_view getName() const override { return "UnreachableCoverBin"; }
    std::string_view getDescription() const override {
        return "Flags coverpoint bins whose values all lie outside the range of the coverpoint expression "
               "(for example bins {[16:31]} on a 4-bit value). Such a bin can never be hit.";
    }
    std::string_view getCategory() const override { return "Coverage"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Warning; }

    engine::RuleTask check(engine::RuleContext& ctx) override {
        BinVisitor visitor(ctx.diags());
        co_await ctx.visitAst(visitor);
    }
};

} // namespace uvmcheck::rules::coverage

namespace uvmcheck::rules {
std::unique_ptr<Rule> createUnreachableCoverBin() {
    return std::make_unique<coverage::UnreachableCoverBin>();
}
} // namespace uvmcheck::rules

UVMCHECK_REGISTER_RULE(uvmcheck::rules::createUnreachableCoverBin)
