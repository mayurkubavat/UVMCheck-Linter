#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "slang/syntax/SyntaxVisitor.h"
#include "slang/syntax/AllSyntax.h"
#include "slang/syntax/SyntaxTree.h"

#include <cctype>
#include <string>
#include <vector>

namespace uvmcheck::rules::sva {

class VacuousAntecedentVisitor : public engine::SyntaxVisitor<VacuousAntecedentVisitor> {
public:
    explicit VacuousAntecedentVisitor(core::DiagnosticEngine& diags) : diags(diags) {}

    void handle(const slang::syntax::BinaryPropertyExprSyntax& propExpr) {
        if (propExpr.kind == slang::syntax::SyntaxKind::ImplicationPropertyExpr) {
            std::string antStr = trim(std::string(propExpr.left->toString()));

            bool isVacuousFalse = false;
            if (antStr == "0" || antStr == "1'b0" || antStr == "'0" || antStr == "false" ||
                antStr == "1'sb0") {
                isVacuousFalse = true;
            } else if (contradictory(antStr)) {
                // `req && !req`; `valid && !ready` is an ordinary condition.
                isVacuousFalse = true;
            }

            if (isVacuousFalse) {
                diags.addViolation(
                    "SVA-VAC-001",
                    "SvaVacuousAntecedent",
                    "Vacuous assertion implication detected: antecedent '" + antStr + "' evaluates to constant false. "
                    "In SVA (IEEE 1800 § 16.12.22), a false antecedent makes the implication evaluate to true on every cycle, "
                    "causing the assertion to pass vacuously 100% of the time and masking unverified DUT logic.",
                    core::Severity::Error,
                    propExpr.left->sourceRange().start()
                );
            }
        }
        visitDefault(propExpr);
    }

private:
    // `e` without parentheses around the whole of it.
    static std::string unparen(std::string e) {
        e = trim(e);
        while (e.size() >= 2 && e.front() == '(' && e.back() == ')') {
            int depth = 0;
            bool whole = true;
            for (size_t i = 0; i < e.size(); i++) {
                if (e[i] == '(') depth++;
                else if (e[i] == ')') depth--;
                if (depth == 0 && i + 1 < e.size()) { whole = false; break; }
            }
            if (!whole) break;
            e = trim(e.substr(1, e.size() - 2));
        }
        return e;
    }

    // One top-level `&&` term is the logical or bitwise negation of another
    // (the native engine's sva_vacuous_antecedent.rs is the same test).
    static bool contradictory(const std::string& ant) {
        std::vector<std::string> terms;
        const std::string e = unparen(ant);
        int depth = 0;
        size_t start = 0;
        for (size_t i = 0; i < e.size(); i++) {
            const char ch = e[i];
            if (ch == '(' || ch == '[' || ch == '{') depth++;
            else if (ch == ')' || ch == ']' || ch == '}') depth--;
            else if (ch == '&' && depth == 0 && i + 1 < e.size() && e[i + 1] == '&') {
                terms.push_back(e.substr(start, i - start));
                start = i + 2;
                i++;
            }
        }
        terms.push_back(e.substr(start));
        auto norm = [](const std::string& x) {
            std::string out;
            for (char ch : unparen(x))
                if (!std::isspace(static_cast<unsigned char>(ch))) out += ch;
            return out;
        };
        std::vector<std::string> pos, neg;
        for (const auto& raw : terms) {
            const std::string t = unparen(raw);
            const bool negated = (!t.empty() && t[0] == '!' && t.rfind("!=", 0) != 0) ||
                                 (!t.empty() && t[0] == '~' && (t.size() < 2 || (t[1] != '&' && t[1] != '|' && t[1] != '^')));
            if (negated) neg.push_back(norm(t.substr(1)));
            else pos.push_back(norm(t));
        }
        for (const auto& p : pos)
            for (const auto& n : neg)
                if (p == n) return true;
        return false;
    }

    static std::string trim(const std::string& str) {
        size_t first = str.find_first_not_of(" \t\r\n");
        if (first == std::string::npos) return "";
        size_t last = str.find_last_not_of(" \t\r\n");
        return str.substr(first, last - first + 1);
    }

    core::DiagnosticEngine& diags;
};

class SvaVacuousAntecedent : public Rule {
public:
    std::string_view getId() const override { return "SVA-VAC-001"; }
    std::string_view getName() const override { return "SvaVacuousAntecedent"; }
    std::string_view getDescription() const override {
        return "Detects constant-false antecedents in assertion implications (0 |-> ...). False antecedents cause "
               "vacuous passes, giving a false 100% pass metric while never verifying DUT behavior.";
    }
    std::string_view getCategory() const override { return "SVA"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Error; }

    engine::RuleTask check(engine::RuleContext& ctx) override {
        auto& diags = ctx.diags();
        VacuousAntecedentVisitor visitor(diags);
        co_await ctx.visitSyntax(visitor);
    }
};

} // namespace uvmcheck::rules::sva

namespace uvmcheck::rules {
std::unique_ptr<Rule> createSvaVacuousAntecedent() {
    return std::make_unique<sva::SvaVacuousAntecedent>();
}
}

UVMCHECK_REGISTER_RULE(uvmcheck::rules::createSvaVacuousAntecedent)
