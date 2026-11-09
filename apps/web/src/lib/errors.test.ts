import { ApiError } from "./api";
import { fieldMessage, messageFor, retryable } from "./errors";

describe("messageFor", () => {
  it("explains a feature the platform switched off for the spa (403 feature.disabled)", () => {
    const e = new ApiError(403, "feature.disabled");
    expect(messageFor(e)).toBe("Tính năng này đang tắt cho spa. Liên hệ bộ phận hỗ trợ.");
    expect(retryable(e)).toBe(false);
  });

  it("falls back to a generic message for an unknown code, and a network message otherwise", () => {
    expect(messageFor(new ApiError(500, "internal"))).toBe("Có lỗi xảy ra. Vui lòng thử lại.");
    expect(messageFor(new TypeError("Failed to fetch"))).toBe("Không kết nối được máy chủ. Vui lòng kiểm tra mạng.");
  });
});

describe("fieldMessage", () => {
  it("maps a field code and falls back for an unknown one", () => {
    const e = new ApiError(422, "validation_failed", [
      { field: "discount", code: "percent_or_amount" },
      { field: "note", code: "something_new" },
    ]);
    expect(fieldMessage(e, "discount")).toBe("Chỉ nhập % hoặc số tiền, không cả hai.");
    expect(fieldMessage(e, "note")).toBe("Giá trị không hợp lệ.");
    expect(fieldMessage(e, "phone")).toBeUndefined();
  });
});
