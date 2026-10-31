#pragma once

#include <string_view>
#include <vector>

namespace uvmcheck::rules {

struct RuleOptionDoc {
    std::string_view key;
    std::string_view defaultValue; // as written in the rule source
};

/// The tested example and options of a rule, compiled in from tests/rules/
/// and the rule sources (scripts/gen_rule_extras.py).
struct RuleExtra {
    std::string_view ruleId;
    std::string_view testDir; // e.g. "tests/rules/correctness/SV-COR-008"
    std::string_view badExample;
    std::string_view goodExample;
    std::vector<RuleOptionDoc> options;
};

const std::vector<RuleExtra>& ruleExtras();

/// The extras for one rule, or nullptr.
const RuleExtra* findRuleExtra(std::string_view ruleId);

} // namespace uvmcheck::rules
