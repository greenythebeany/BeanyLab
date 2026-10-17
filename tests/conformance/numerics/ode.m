% ode45/ode23 on problems with known solutions.
[t, y] = ode45(@(t, y) -2*y, [0 1], 1);
fprintf('ode45 exp decay: %.5f (exact %.5f)\n', y(end), exp(-2));
opts = odeset('RelTol', 1e-8, 'AbsTol', 1e-10);
[t, y] = ode45(@(t, y) [y(2); -y(1)], [0 pi], [0; 1], opts);
fprintf('ode45 oscillator: |sin(pi)| < 1e-6: %d, cos(pi) = %.6f\n', abs(y(end, 1)) < 1e-6, y(end, 2));
[t, y] = ode23(@(t, y) t, [0 2], 0);
fprintf('ode23 integral of t: %.4f\n', y(end));
[t, y] = ode45(@(t, y) -y, [0 0.5 1], 1);
fprintf('requested times: %s\n', mat2str(t'));
fprintf('values: %.4f %.4f %.4f\n', y);
