function [total, avg] = my_function(values, scale)
% MY_FUNCTION  Sum and average of scaled values.
%   [total, avg] = my_function(values) uses scale 1.
%   [total, avg] = my_function(values, scale) multiplies first.

if nargin < 2
    scale = 1;
end
scaled = values * scale;
total = sum(scaled);
avg = mean(scaled);
end
