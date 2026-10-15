% Calls a function file in the same folder and a local function.
[m, s] = helper_stats([1 2 3])
v = helper_stats([2 4], 10)
y = square(3) + square(4)

function r = square(x)
    r = x^2;
end
