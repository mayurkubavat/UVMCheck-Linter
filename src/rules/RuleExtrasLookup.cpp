#include "uvmcheck/rules/RuleExtras.h"

namespace uvmcheck::rules {

const RuleExtra* findRuleExtra(std::string_view ruleId) {
    for (const auto& e : ruleExtras()) {
        if (e.ruleId == ruleId) return &e;
    }
    return nullptr;
}

} // namespace uvmcheck::rules
