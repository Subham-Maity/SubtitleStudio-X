import { create } from "zustand";
import type { AppRoute } from "@/types/studio";

interface AppStoreState {
  route: AppRoute;
  selectedLanguage: string;
  modelsModalOpen: boolean;
  setRoute: (route: AppRoute) => void;
  setSelectedLanguage: (lang: string) => void;
  setModelsModalOpen: (open: boolean) => void;
}

export const useAppStore = create<AppStoreState>((set) => ({
  route: { view: "home" },
  selectedLanguage: "hi",
  modelsModalOpen: false,
  setRoute: (route) => set({ route }),
  setSelectedLanguage: (selectedLanguage) => set({ selectedLanguage }),
  setModelsModalOpen: (modelsModalOpen) => set({ modelsModalOpen }),
}));

