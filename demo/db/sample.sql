CREATE TABLE `users` (
  `id` INT NOT NULL,
  `email` VARCHAR(255),
  `status` VARCHAR(32)
);
INSERT INTO `users` VALUES (1,'a@example.com','ok'),(2,'b@example.com','failed');
INSERT INTO `users` VALUES (3,'c@example.com','ok');
CREATE TABLE `orders` (
  `id` INT NOT NULL,
  `user_id` INT,
  `status` VARCHAR(32)
);
INSERT INTO `orders` VALUES (1,1,'done'),(2,2,'failed');
