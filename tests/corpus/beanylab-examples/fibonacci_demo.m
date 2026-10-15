% Script with a local function, loops, structs and cells.
n = 12;
seq = zeros(1, n);
for k = 1:n
    seq(k) = fib(k);
end
disp(seq)

people = struct('name', {'Ada', 'Linus', 'Grace'}, 'age', {36, 28, 85});
for p = people
    fprintf('%-6s is %d years old\n', p.name, p.age);
end

words = {'matrix', 'vector', 'scalar'};
lens = cellfun(@length, words);
[~, longest] = max(lens);
fprintf('Longest word: %s\n', words{longest});

function f = fib(k)
    if k <= 2
        f = 1;
    else
        f = fib(k - 1) + fib(k - 2);
    end
end
