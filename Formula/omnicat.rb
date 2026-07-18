# frozen_string_literal: true

class Omnicat < Formula
  desc "Universal file preview for terminal and GUI — a context-aware cat"
  homepage "https://github.com/s00d/omnicat"
  license "MIT"
  version "0.8.0"

  head do
    url "https://github.com/s00d/omnicat.git", branch: "main"
  end

  depends_on "rust" => :build

  def install
    system "cargo", "install", *std_cargo_args
  end

  test do
    assert_match "omnicat", shell_output("#{bin}/omnicat --version")
  end
end
