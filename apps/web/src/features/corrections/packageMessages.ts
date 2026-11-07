import { addMessages } from "../../lib/errors";

// Field refusals only the package extend form can get. They live with that lazy form
// (PackageActions), not in the startup chunk (frontend.md "Perf budget").
addMessages(
  {},
  {
    not_later: "Hạn mới phải sau hạn hiện tại.",
    package_has_no_expiry: "Liệu trình không có thời hạn, không cần gia hạn.",
  },
);
