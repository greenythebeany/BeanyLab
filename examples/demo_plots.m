%% Signals
% A short tour of plotting. Run the whole file with F5,
% or one section at a time with Ctrl+Enter.
t = linspace(0, 2*pi, 200);
figure;
plot(t, sin(t), 'LineWidth', 2);
hold on
plot(t, cos(t), '--');
hold off
grid on
title('Sine and cosine')
xlabel('t (rad)')
ylabel('amplitude')
legend('sin', 'cos')

%% Surface
[X, Y] = meshgrid(-2:0.1:2);
Z = X .* exp(-X.^2 - Y.^2);
figure;
surf(X, Y, Z)
title('z = x e^{-x^2-y^2}')
colorbar

%% Statistics
data = randn(1, 1000);
figure;
subplot(1, 2, 1)
histogram(data, 30)
title('Normal samples')
subplot(1, 2, 2)
bar([3 7 2 9 4])
title('Bar chart')

%% Calling a function file
[s, m] = my_function([1 2 3 4], 2);
fprintf('sum = %g, mean = %g\n', s, m);
