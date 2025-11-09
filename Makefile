.PHONY: help android-setup android-release android-debug android-install clean-android

# Config
KEYSTORE_FILE ?= upload-keystore.jks
KEYSTORE_ALIAS ?= upload
KEY_PASSWORD ?= $(ANDROID_KEY_PASSWORD)
ANDROID_DIR = src-tauri/gen/android
KEYSTORE_PROPS = $(ANDROID_DIR)/keystore.properties

help:
	@echo "Android Build Targets:"
	@echo "  android-setup      - Generate keystore (run once)"
	@echo "  android-release    - Build signed release APK"
	@echo "  android-debug      - Build debug APK"
	@echo "  android-install    - Install debug APK to device"
	@echo "  clean-android      - Clean Android build artifacts"
	@echo ""
	@echo "Environment Variables:"
	@echo "  ANDROID_KEY_PASSWORD   - Keystore password"
	@echo "  KEYSTORE_FILE          - Keystore path (default: upload-keystore.jks)"

android-setup:
	@echo "Generating keystore..."
	@if [ -f "$(KEYSTORE_FILE)" ]; then \
		echo "Keystore already exists at $(KEYSTORE_FILE)"; \
		exit 1; \
	fi
	keytool -genkey -v -keystore $(KEYSTORE_FILE) \
		-keyalg RSA -keysize 2048 -validity 10000 -alias $(KEYSTORE_ALIAS)
	@echo ""
	@echo "Keystore created: $(KEYSTORE_FILE)"
	@echo ""
	@echo "Next steps:"
	@echo "1. export ANDROID_KEY_PASSWORD=your_password"
	@echo "2. make android-release"

android-release: check-keystore check-password
	@echo "Creating keystore.properties..."
	@mkdir -p $(ANDROID_DIR)
	@echo "password=$(KEY_PASSWORD)" > $(KEYSTORE_PROPS)
	@echo "keyAlias=$(KEYSTORE_ALIAS)" >> $(KEYSTORE_PROPS)
	@echo "storeFile=$(PWD)/$(KEYSTORE_FILE)" >> $(KEYSTORE_PROPS)
	@echo "Building signed release APK..."
	pnpm run tauri android build
	@rm -f $(KEYSTORE_PROPS)
	@echo ""
	@echo "Release APK: $(ANDROID_DIR)/app/build/outputs/apk/universal/release/app-universal-release.apk"

android-debug:
	@echo "Building debug APK..."
	pnpm run tauri android build

android-install: android-debug
	@echo "Installing debug APK..."
	adb install -r $(ANDROID_DIR)/app/build/outputs/apk/debug/app-debug.apk

clean-android:
	@echo "Cleaning Android build..."
	@cd $(ANDROID_DIR) && ./gradlew clean
	@rm -rf $(ANDROID_DIR)/app/build
	@rm -f $(KEYSTORE_PROPS)

check-keystore:
	@if [ ! -f "$(KEYSTORE_FILE)" ]; then \
		echo "Error: Keystore not found at $(KEYSTORE_FILE)"; \
		echo "Run 'make android-setup' first"; \
		exit 1; \
	fi

check-password:
	@if [ -z "$(KEY_PASSWORD)" ]; then \
		echo "Error: ANDROID_KEY_PASSWORD not set"; \
		exit 1; \
	fi
