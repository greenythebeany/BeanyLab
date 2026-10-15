function [m, s] = helper_stats(x, w)
% HELPER_STATS  Mean (optionally weighted) and sum, used by conformance tests.
if nargin < 2
    w = 1;
end
m = mean(x) * w;
s = sum(x);
end
