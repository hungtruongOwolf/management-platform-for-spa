import { describe, expect, it } from "vitest";
import { ApiError } from "../../lib/api";
import { messageFor } from "../../lib/errors";
import "./mfaMessages";

describe("second-factor messages (registered by /xac-thuc and the platform console)", () => {
  it("explains a wrong or reused code", () => {
    expect(messageFor(new ApiError(401, "auth.invalid_mfa_code"))).toBe("Mã không đúng, đã hết hạn hoặc đã dùng. Đợi mã mới rồi thử lại.");
  });
});
